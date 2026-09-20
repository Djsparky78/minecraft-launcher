use super::*;
use model::*;
use sha1::{Digest, Sha1};
use std::{
    io::Cursor,
    net::TcpListener,
    sync::{atomic::AtomicUsize, Arc},
    thread,
    time::Duration,
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}
fn download(url: &str, bytes: &[u8]) -> Download {
    Download {
        url: url.into(),
        sha1: Some(hash(bytes)),
        size: Some(bytes.len() as u64),
        path: String::new(),
    }
}
fn platform() -> Platform {
    Platform::windows("amd64", "10.0.22631")
}
struct Server {
    url: String,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(routes: BTreeMap<String, Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicUsize::new(0));
        let s = stop.clone();
        let c = count.clone();
        let handle = thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                if let Ok((mut socket, _)) = listener.accept() {
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 8192];
                    if let Ok(n) = socket.read(&mut request) {
                        let text = String::from_utf8_lossy(&request[..n]);
                        let path = text.split_whitespace().nth(1).unwrap_or("");
                        c.fetch_add(1, Ordering::Relaxed);
                        if let Some(body) = routes.get(path) {
                            let _ = write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                            let _ = socket.write_all(body);
                        } else {
                            let _ = write!(socket, "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        }
                    }
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Self {
            url,
            count,
            stop,
            handle: Some(handle),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}
fn zip_bytes(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap().into_inner()
}
#[test]
fn rule_order_os_arch_version_and_features() {
    let rules: Vec<Rule> = serde_json::from_value(serde_json::json!([
        {"action":"allow"}, {"action":"disallow","os":{"name":"osx"}},
        {"action":"disallow","os":{"arch":"x86$"}},
        {"action":"disallow","os":{"version":"^6\\."}},
        {"action":"disallow","features":{"is_demo_user":true}}
    ]))
    .unwrap();
    assert!(allowed(None, &platform()).unwrap());
    assert!(!allowed(Some(&[]), &platform()).unwrap());
    assert!(allowed(Some(&rules), &platform()).unwrap());
    assert!(!allowed(Some(&rules), &Platform::windows("x86", "10.0")).unwrap());
    assert!(!allowed(Some(&rules), &Platform::windows("amd64", "6.1")).unwrap());
    let rules: Vec<Rule> = serde_json::from_value(
        serde_json::json!([{"action":"disallow"},{"action":"allow","os":{"name":"windows"}}]),
    )
    .unwrap();
    assert!(allowed(Some(&rules), &platform()).unwrap());
}
fn metadata(libraries: serde_json::Value) -> Metadata {
    serde_json::from_value(serde_json::json!({"id":"1.20.1","downloads":{"client":{"url":"https://piston-data.mojang.com/client","sha1":hash(b"client"),"size":6}},"assetIndex":{"id":"test","url":"https://piston-meta.mojang.com/index"},"libraries":libraries})).unwrap()
}
#[test]
fn plans_only_windows_and_correct_architecture_natives() {
    let artifact = |path: &str| serde_json::json!({"path":path,"url":format!("https://libraries.minecraft.net/{path}")});
    let m = metadata(serde_json::json!([
        {"name":"a:base:1","downloads":{"artifact":artifact("base.jar")}},
        {"name":"a:native:1:natives-linux","rules":[{"action":"allow","os":{"name":"linux"}}],"downloads":{"artifact":artifact("linux.jar")}},
        {"name":"a:native:1:natives-windows-arm64","rules":[{"action":"allow","os":{"name":"windows"}}],"downloads":{"artifact":artifact("arm.jar")}},
        {"name":"a:native:1:natives-windows","rules":[{"action":"allow","os":{"name":"windows"}}],"downloads":{"artifact":artifact("win.jar")}},
        {"name":"old:native:1","natives":{"windows":"natives-windows-${arch}"},"extract":{"exclude":["META-INF/"]},"downloads":{"classifiers":{"natives-windows-64":artifact("old.jar"),"natives-linux":artifact("no.jar")}}}
    ]));
    let plan = plan(&m, &platform()).unwrap();
    assert_eq!(plan.natives.len(), 2);
    assert_eq!(plan.tasks.len(), 4);
    assert!(!plan
        .tasks
        .iter()
        .any(|t| t.path.contains("linux") || t.path.contains("arm") || t.path.contains("no.jar")));
}
#[test]
fn asset_plan_deduplicates_content_and_validates_hashes() {
    let index: AssetIndex=serde_json::from_value(serde_json::json!({"objects":{"a":{"hash":hash(b"asset"),"size":5},"b":{"hash":hash(b"asset"),"size":5}},"virtual":true,"map_to_resources":true})).unwrap();
    let tasks = asset_tasks(&index).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].download.size, Some(5));
    let index: AssetIndex =
        serde_json::from_value(serde_json::json!({"objects":{"a":{"hash":"bad","size":1}}}))
            .unwrap();
    assert!(asset_tasks(&index).is_err());
}
#[test]
fn hash_and_size_verification_rejects_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let task = task("test.jar".into(), "Client", download("", b"abcd")).unwrap();
    assert!(storage::verify(dir.path(), &task, &cancel)
        .unwrap()
        .is_none());
    fs::write(dir.path().join("test.jar"), b"abcd").unwrap();
    assert!(storage::verify(dir.path(), &task, &cancel)
        .unwrap()
        .is_some());
    fs::write(dir.path().join("test.jar"), b"abce").unwrap();
    assert!(storage::verify(dir.path(), &task, &cancel)
        .unwrap()
        .is_none());
    fs::write(dir.path().join("test.jar"), b"abc").unwrap();
    assert!(storage::verify(dir.path(), &task, &cancel)
        .unwrap()
        .is_none());
}
#[test]
fn unsafe_paths_and_zip_traversal_are_rejected() {
    for path in [
        "../evil",
        "/absolute",
        "C:/escape",
        "a\\b",
        "CON.txt",
        "x/../z",
        "file:stream",
        "LPT1",
        "x//y",
    ] {
        assert!(safe_relative(path).is_err(), "{path}");
    }
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("native.jar");
    fs::write(&archive, zip_bytes("../escape.dll", b"dll")).unwrap();
    assert!(extract_native(&archive, dir.path(), &[], &AtomicBool::new(false)).is_err());
}
#[test]
fn extraction_honors_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("native.jar");
    fs::write(&archive, zip_bytes("META-INF/MANIFEST.MF", b"skip")).unwrap();
    extract_native(&archive, dir.path(), &[], &AtomicBool::new(false)).unwrap();
    assert!(!dir.path().join("META-INF").exists());
}
#[test]
fn failed_download_does_not_publish_partial_file_and_cancellation_works() {
    let server = Server::new(BTreeMap::from([("/bad".into(), b"wrong".to_vec())]));
    let dir = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir_in(dir.path()).unwrap();
    let task = task(
        "client.jar".into(),
        "Client",
        download(&format!("{}/bad", server.url), b"right"),
    )
    .unwrap();
    let downloader = Downloader::new().unwrap();
    assert!(downloader
        .ensure(
            dir.path(),
            &task,
            staging.path(),
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err());
    assert!(!dir.path().join("client.jar").exists());
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
    assert!(matches!(
        downloader.ensure(
            dir.path(),
            &task,
            staging.path(),
            &AtomicBool::new(true),
            |_| {}
        ),
        Err(Error::Cancelled)
    ));
}
#[test]
fn installer_lock_blocks_duplicate_and_shared_library_writes() {
    let dir = tempfile::tempdir().unwrap();
    let first = acquire_lock(dir.path()).unwrap();
    assert!(acquire_lock(dir.path()).is_err());
    drop(first);
    assert!(acquire_lock(dir.path()).is_ok());
}
#[test]
fn installation_state_and_repair_reuse_verified_files_and_fix_damage() {
    let native = zip_bytes("test.dll", b"native");
    let index = b"{\"objects\":{}}";
    let server = Server::new(BTreeMap::from([
        ("/client".into(), b"client".to_vec()),
        ("/lib".into(), b"library".to_vec()),
        ("/native".into(), native.clone()),
        ("/index".into(), index.to_vec()),
    ]));
    let d = |path: &str, bytes: &[u8]| {
        serde_json::to_value(download(&format!("{}{path}", server.url), bytes)).unwrap()
    };
    let mut lib = d("/lib", b"library");
    lib["path"] = "test/lib.jar".into();
    let mut nat = d("/native", &native);
    nat["path"] = "test/native.jar".into();
    let mut idx = d("/index", index);
    idx["id"] = "test".into();
    let metadata=serde_json::to_vec(&serde_json::json!({"id":"test","downloads":{"client":d("/client",b"client")},"assetIndex":idx,"libraries":[{"name":"test:lib:1","downloads":{"artifact":lib}},{"name":"test:native:1","natives":{"windows":"natives-windows"},"downloads":{"classifiers":{"natives-windows":nat}}}]})).unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("versions/test")).unwrap();
    fs::write(dir.path().join("versions/test/test.json"), &metadata).unwrap();
    let metadata_download = download("https://piston-meta.mojang.com/not-needed", &metadata);
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    install(
        dir.path(),
        "test",
        metadata_download.clone(),
        platform(),
        &cancel,
        |p| events.push(p),
    )
    .unwrap();
    assert_eq!(list(dir.path()).unwrap()[0].status, "installed");
    assert_eq!(events.last().unwrap().percent, 100.0);
    assert_eq!(server.count.load(Ordering::Relaxed), 4);
    install(
        dir.path(),
        "test",
        metadata_download.clone(),
        platform(),
        &cancel,
        |_| {},
    )
    .unwrap();
    assert_eq!(server.count.load(Ordering::Relaxed), 4);
    fs::write(dir.path().join("versions/test/test.jar"), b"broken").unwrap();
    fs::remove_file(dir.path().join("natives/test/test.dll")).unwrap();
    assert_eq!(list(dir.path()).unwrap()[0].status, "needs_repair");
    install(
        dir.path(),
        "test",
        metadata_download.clone(),
        platform(),
        &cancel,
        |_| {},
    )
    .unwrap();
    assert_eq!(server.count.load(Ordering::Relaxed), 5);
    assert_eq!(
        fs::read(dir.path().join("natives/test/test.dll")).unwrap(),
        b"native"
    );
    assert_eq!(list(dir.path()).unwrap()[0].status, "installed");
    cancel.store(true, Ordering::Relaxed);
    assert!(matches!(
        install(
            dir.path(),
            "test",
            metadata_download,
            platform(),
            &cancel,
            |_| {}
        ),
        Err(Error::Cancelled)
    ));
    assert_eq!(list(dir.path()).unwrap()[0].status, "cancelled");
}

#[test]
fn parses_official_modern_and_legacy_metadata() {
    for text in [
        include_str!("../tests/fixtures/1.20.1.json"),
        include_str!("../tests/fixtures/1.6.4.json"),
    ] {
        let metadata: Metadata = serde_json::from_str(text).unwrap();
        let plan = plan(&metadata, &platform()).unwrap();
        assert!(!plan.natives.is_empty());
        assert!(plan.tasks.iter().any(|t| t.stage == "Client"));
        assert!(plan.tasks.iter().all(|t| !t.path.contains("natives-linux")
            && !t.path.contains("natives-macos")
            && !t.path.contains("natives-osx")
            && !t.path.contains("natives-windows-arm64")
            && !t.path.contains("natives-windows-x86")));
    }
}
#[test]
fn legacy_asset_materialization_and_repair() {
    let root = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir_in(root.path()).unwrap();
    let cancel = AtomicBool::new(false);
    fs::write(root.path().join("object"), b"asset").unwrap();
    let source = VerifiedFile {
        path: "object".into(),
        sha1: hash(b"asset"),
        size: 5,
    };
    for destination in [
        "assets/virtual/legacy/sound/test.ogg",
        "versions/test/resources/sound/test.ogg",
    ] {
        storage::materialize(root.path(), &source, destination, staging.path(), &cancel).unwrap();
        assert_eq!(fs::read(root.path().join(destination)).unwrap(), b"asset");
        fs::write(root.path().join(destination), b"wrong").unwrap();
        storage::materialize(root.path(), &source, destination, staging.path(), &cancel).unwrap();
        assert_eq!(fs::read(root.path().join(destination)).unwrap(), b"asset");
    }
}
#[test]
fn display_paths_remove_windows_prefixes_without_changing_unc() {
    assert_eq!(
        display_path(Path::new(r"\\?\C:\Java\bin\java.exe")),
        r"C:\Java\bin\java.exe"
    );
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\server\share\java.exe")),
        r"\\server\share\java.exe"
    );
}
#[test]
fn interrupted_and_corrupt_receipts_are_not_ready() {
    let dir = tempfile::tempdir().unwrap();
    let receipt = Receipt {
        schema: 1,
        version: "test".into(),
        status: "installing".into(),
        error: None,
        files: Vec::new(),
        platform: platform(),
    };
    atomic_json(dir.path(), &receipt_path("test"), &receipt).unwrap();
    assert_eq!(list(dir.path()).unwrap()[0].status, "incomplete");
    fs::write(dir.path().join(receipt_path("test")), b"invalid json").unwrap();
    assert_eq!(list(dir.path()).unwrap()[0].status, "needs_repair");
}
