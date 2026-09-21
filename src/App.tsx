import { useAccount } from "./hooks/useAccount";
import { AccountSettings } from "./components/AccountSettings";
import { useEffect, useState } from "react";
import { useInstaller } from "./hooks/useInstaller";
import { invoke } from "@tauri-apps/api/core";
import { Box, ChevronDown, Download, Gamepad2, Settings, UserRound } from "lucide-react";

interface VersionList { latest_release: string; versions: { id: string; url: string }[]; cached: boolean; warning: string | null }
interface JavaInstallation { path: string; version: string; major_version: number; vendor: string | null }

function App() {
  const [page, setPage] = useState<"play" | "settings" | "installations">("play");
  const installer = useInstaller();
  const auth = useAccount();
  const [latestRelease, setLatestRelease] = useState("");
  const [version, setVersion] = useState("");
  const selectedInstall = installer.installations.find(item => item.version === version);
  const [versions, setVersions] = useState<VersionList["versions"]>([]);
  const [loading, setLoading] = useState(true);
  const [versionMessage, setVersionMessage] = useState("");
  const [java, setJava] = useState<JavaInstallation[]>([]);
  const [javaLoading, setJavaLoading] = useState(true);
  const [javaError, setJavaError] = useState("");

  async function loadVersions() {
    setLoading(true);
    setVersionMessage("");
    try {
      const result = await invoke<VersionList>("minecraft_versions");
      setVersions(result.versions);
      setLatestRelease(result.latest_release);
      setVersion(current => result.versions.some(v => v.id === current) ? current : result.latest_release);
      setVersionMessage(result.warning ?? (result.cached ? "Showing cached versions" : "Versions updated from Mojang"));
    } catch (error) {
      setVersionMessage(String(error));
    } finally { setLoading(false); }
  }

  async function scanJava() {
    setJavaLoading(true);
    setJavaError("");
    try { setJava(await invoke<JavaInstallation[]>("detect_java")); }
    catch (error) { setJavaError(String(error)); }
    finally { setJavaLoading(false); }
  }

  useEffect(() => { void loadVersions(); void scanJava(); }, []);

  return (
    <main className="shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><Box size={21} strokeWidth={2.4} /></div>
          <div><strong>EMBER</strong><span>LAUNCHER</span></div>
        </div>

        <nav aria-label="Launcher navigation">
          <button onClick={() => setPage("play")} aria-current={page === "play" ? "page" : undefined} className={`nav-item ${page === "play" ? "active" : ""}`}><Gamepad2 size={19} /> Play</button>
          <button onClick={() => setPage("installations")} aria-current={page === "installations" ? "page" : undefined} className={`nav-item ${page === "installations" ? "active" : ""}`}><Download size={19} /> Installations</button>
          <button onClick={() => setPage("settings")} aria-current={page === "settings" ? "page" : undefined} className={`nav-item ${page === "settings" ? "active" : ""}`}><Settings size={19} /> Settings</button>
        </nav>

        <div className="account">
          <div className="avatar"><UserRound size={19} /></div>
          <div><strong>{auth.account.profile?.name ?? "Guest player"}</strong><span>{auth.account.status === "signed_in" ? "Minecraft account" : "Not signed in"}</span>
            <button className="account-link" onClick={() => setPage("settings")}>{auth.account.profile ? "Manage account" : "Microsoft sign-in"}</button></div>
        </div>
      </aside>

      <section className="content">
        <div hidden={page !== "play"}>
        <header>
          <div>
            <p className="eyebrow">MINECRAFT: JAVA EDITION</p>
            <h1>Your next world<br />starts here.</h1>
          </div>
          <div className="status-pill"><i /> {installer.checking ? "Checking installations…" : selectedInstall?.status === "installed" ? "Installed and ready" : "Ready to install"}</div>
        </header>

        <section className="hero">
          <div className="glow glow-one" />
          <div className="glow glow-two" />
          <div className="blocks" aria-hidden="true">
            <span className="block block-a" />
            <span className="block block-b" />
            <span className="block block-c" />
          </div>
          <div className="hero-copy">
            <span className="tag">JAVA EDITION</span>
            <h2>Build. Explore.<br />Survive.</h2>
            <p>A fast, focused launcher for getting into the game.</p>
          </div>
        </section>

        <section className="launch-card">
          <div className="version-wrap">
            <label htmlFor="version">VERSION</label>
            <div className="select-wrap">
              <select id="version" disabled={loading || !versions.length || !!installer.activeVersion} value={version} onChange={(event) => setVersion(event.target.value)}>
                {!versions.length && <option value="">{loading ? "Loading releases…" : "No versions available"}</option>}
                {versions.map((item) => <option key={item.id} value={item.id}>{item.id}{item.id === latestRelease ? " (Latest release)" : ""}</option>)}
              </select>
              <ChevronDown size={18} />
            </div>
          </div>
          <button className="play-button" onClick={() => void installer.install(version)} disabled={!!installer.activeVersion || installer.checking || loading || !version}>
            {installer.activeVersion ? "WORKING…" : selectedInstall?.status === "installed" ? "PLAY" : selectedInstall ? "RESUME / REPAIR" : "INSTALL"}
          </button>
        </section>

        <section className="environment" aria-label="Launcher environment">
          <p role="status">{loading ? "Loading Minecraft releases…" : versionMessage}</p>
          <button onClick={() => void loadVersions()} disabled={loading}>Refresh versions</button>
        </section>
        </div>
        <section className="environment" hidden={page !== "installations"} aria-label="Installations">
          <h1>Installations</h1>
          <p>Vanilla Minecraft files managed by Ember. Game launching comes in a later milestone.</p>
          <button onClick={() => void installer.refresh().catch(() => {})} disabled={installer.checking || !!installer.activeVersion}>Recheck installations</button>
          {installer.checking && <p role="status">Verifying installed files…</p>}
          {!installer.checking && !installer.installations.length && <p>No installed versions yet. Select a release on Play and choose Install.</p>}
          <ul className="installation-list">{installer.installations.map(item => <li key={item.version}>
            <h2>Minecraft {item.version}</h2>
            <p>Status: {installer.activeVersion === item.version ? "Installing / repairing" : item.status.replace(/_/g, " ")}</p>
            <code>{item.location}</code>
            {item.error && <p>{item.error}</p>}
            <button onClick={() => void installer.install(item.version, true)} disabled={!!installer.activeVersion || installer.checking}>Repair / reverify {item.version}</button>
          </li>)}</ul>
        </section>
        {(installer.message || installer.error || installer.activeVersion || installer.listError) && <section className="environment" aria-label="Installation progress">
          <p role="status">{installer.message}</p>
          {installer.progress && <>
            <p>{installer.progress.version}: {installer.progress.stage}</p>
            <progress aria-label="Install progress" max={100} value={installer.progress.percent} />
            <p>{Math.floor(installer.progress.percent)}% · {installer.progress.completed_files} / {installer.progress.total_files} files</p>
            <code>{installer.progress.file}</code>
            {installer.progress.file_total != null && <p>{(installer.progress.file_bytes / 1048576).toFixed(1)} / {(installer.progress.file_total / 1048576).toFixed(1)} MB</p>}
          </>}
          {installer.error && <p role="alert">{installer.error}</p>}
          {installer.listError && <p role="alert">Could not check installations: {installer.listError}</p>}
          {installer.activeVersion && <button onClick={() => void installer.cancel()} disabled={installer.cancelling}>{installer.cancelling ? "Cancelling…" : "Cancel installation"}</button>}
          {installer.cancelling && <p>A stalled network request may take up to 60 seconds to stop. Completed verified files are retained.</p>}
        </section>}
        <section className="environment" hidden={page !== "settings"} aria-label="Settings">
          <h1>Settings</h1>
          <AccountSettings auth={auth} />
          <h2>Detected Java installations</h2>
          <div aria-live="polite">
            {javaLoading ? <p>Detecting Java…</p> : javaError ? <p role="alert">{javaError}</p> : java.length ?
              <ul>{java.map(item => <li key={item.path}><strong>Java {item.major_version}</strong><span> · {item.vendor ?? "Unknown vendor"} · {item.version}</span><code>{item.path}</code></li>)}</ul> :
              <p>No Java found. Install Java or set JAVA_HOME, then scan again.</p>}
          </div>
          <button onClick={() => void scanJava()} disabled={javaLoading}>Scan for Java</button>
          <p>Java is not required to download game files. Java compatibility and game launching come later.</p>
        </section>

        <footer>
          <span>Launcher v0.1.0</span>
          <span>Not affiliated with Mojang or Microsoft.</span>
        </footer>
      </section>
    </main>
  );
}

export default App;
