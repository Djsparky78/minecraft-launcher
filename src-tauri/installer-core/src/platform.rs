//! Windows platform metadata used by Mojang's library rules.
//! Keep the launcher process architecture: an x64 launcher on ARM64 Windows
//! still needs x64 libraries. OS marketing names (Windows 10/11) are not versions.
use crate::model::Platform;

#[cfg(any(windows, test))]
fn architecture(arch: &str) -> Result<&str, String> {
    match arch {
        "x86_64" => Ok("amd64"),
        "x86" => Ok("x86"),
        "aarch64" => Ok("aarch64"),
        _ => Err(format!("Cannot install Minecraft: unsupported launcher architecture '{arch}'. Expected x64, x86, or ARM64.")),
    }
}

#[cfg(any(windows, test))]
fn version_string(major: u32, minor: u32, build: u32) -> Result<String, String> {
    if major == 0 || build == 0 {
        return Err(format!("Windows RtlGetVersion returned invalid version fields: major={major}, minor={minor}, build={build}. Cannot evaluate Minecraft library rules."));
    }
    // Windows 11 reports a 10.0 kernel version, just like Windows 10. Preserve
    // these values verbatim rather than inferring a version from a product name.
    Ok(format!("{major}.{minor}.{build}"))
}

#[cfg(any(windows, test))]
fn check_status(status: i32) -> Result<(), String> {
    if status < 0 {
        Err(format!("Windows RtlGetVersion failed with NTSTATUS 0x{:08X}. Cannot determine the OS version required by Minecraft library rules.", status as u32))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn windows_version() -> Result<String, String> {
    use windows_sys::{
        Wdk::System::SystemServices::RtlGetVersion,
        Win32::System::SystemInformation::OSVERSIONINFOW,
    };
    // windows-sys binds this API to the user-mode ntdll.dll export. No command
    // process, localized text, environment variable, or version-helper manifest
    // assumptions are involved.
    // SAFETY: OSVERSIONINFOW contains only integer fields and a WCHAR array;
    // zero initialization is valid, and its size field is set before the call.
    let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
    info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
    // SAFETY: info is a correctly sized, initialized, writable structure that
    // remains alive for the entire synchronous call.
    let status = unsafe { RtlGetVersion(&mut info) };
    check_status(status)?;
    version_string(info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber)
}

pub fn detect() -> Result<Platform, String> {
    #[cfg(windows)]
    {
        let arch = architecture(std::env::consts::ARCH)?;
        let version = windows_version()?;
        Ok(Platform::windows(arch, &version))
    }
    #[cfg(not(windows))]
    Err("Minecraft installation currently supports Windows only.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_windows_10_and_windows_11_kernel_versions() {
        assert_eq!(version_string(10, 0, 19045).unwrap(), "10.0.19045");
        assert_eq!(version_string(10, 0, 26100).unwrap(), "10.0.26100");
        assert_eq!(version_string(12, 3, 40000).unwrap(), "12.3.40000");
    }
    #[test]
    fn rejects_invalid_native_fields_and_reports_ntstatus() {
        assert!(version_string(0, 0, 26100).unwrap_err().contains("major=0"));
        assert!(version_string(10, 0, 0).unwrap_err().contains("build=0"));
        assert!(check_status(0).is_ok());
        assert!(check_status(0xC000000Du32 as i32)
            .unwrap_err()
            .contains("0xC000000D"));
    }
    #[test]
    fn preserves_launcher_architecture_mapping() {
        assert_eq!(architecture("x86_64").unwrap(), "amd64");
        assert_eq!(architecture("x86").unwrap(), "x86");
        assert_eq!(architecture("aarch64").unwrap(), "aarch64");
        assert!(architecture("unknown").unwrap_err().contains("unknown"));
    }
    #[test]
    fn native_version_format_matches_mojang_windows_rules() {
        let rules = serde_json::from_str::<Vec<crate::model::Rule>>(
            r#"[{"action":"allow","os":{"name":"windows","version":"^10\\.0\\.[0-9]+$"}}]"#,
        )
        .unwrap();
        for build in [19045, 26100] {
            let platform = Platform::windows("amd64", &version_string(10, 0, build).unwrap());
            assert!(crate::model::allowed(Some(&rules), &platform).unwrap());
        }
    }
    // This is deliberately a runtime test, not just a compilation check. It
    // exercises the same detection path used by the Install button on Windows.
    #[cfg(windows)]
    #[test]
    fn detects_real_windows_version_without_a_shell() {
        let platform = detect().expect("native Windows platform detection must succeed");
        eprintln!("Detected Windows {} ({})", platform.version, platform.arch);
        assert_eq!(platform.arch, architecture(std::env::consts::ARCH).unwrap());
        let parts: Vec<u32> = platform
            .version
            .split('.')
            .map(|part| part.parse().unwrap())
            .collect();
        assert_eq!(parts.len(), 3);
        assert!(parts[0] > 0 && parts[2] > 0);
    }
}
