# Ember Launcher

A lightweight Minecraft launcher foundation built with Tauri 2, React, TypeScript, and Rust.

## What is included

- A polished desktop launcher window
- Version selection and launch-state UI
- React-to-Rust communication through a Tauri command
- Responsive layout for compact desktop windows
- Tauri bundle configuration for Windows, macOS, and Linux

## Windows development setup

Install these prerequisites before running the desktop app:

1. Install the current Node.js LTS release from [nodejs.org](https://nodejs.org/).
2. Install [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/). In the installer, select **Desktop development with C++**.
3. Install Rust with the MSVC toolchain:

   ```powershell
   winget install --id Rustlang.Rustup
   rustup default stable-msvc
   ```

4. Make sure [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) is installed. It is normally already present on current Windows 10 and Windows 11 systems.
5. Restart PowerShell or Windows after installing the prerequisites so the new commands are available.

Tauri's full, current platform instructions are in the [official prerequisites guide](https://v2.tauri.app/start/prerequisites/). Building an MSI may also require the Windows VBSCRIPT optional feature because this project currently bundles all supported installer targets.

Verify your setup in PowerShell:

```powershell
node --version
npm --version
rustc --version
cargo --version
```

## Run locally

### Start the desktop app

```powershell
npm install
npm run tauri:dev
```

### Check the web interface only

```powershell
npm run dev
```

Open the local URL printed by Vite. Browser mode is useful for styling, but the Play button's Rust command only works inside the Tauri desktop window.

## Verification commands

```powershell
npm install
npm test
npm run build
npx tsc --noEmit
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --workspace --manifest-path src-tauri/Cargo.toml
npm run tauri:build
```

The installer is a separate Rust crate, so its tests can run on Linux without
Tauri's GTK/WebKit system libraries:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml -p installer-core
```

## Vanilla installation (milestone 3)

Select a release on Play and click **Install**. Rust resolves its URL and SHA-1
from the official Mojang version manifest, verifies the version JSON, then installs:

- The client JAR and Windows libraries selected by Mojang's ordered allow/disallow
  rules (OS, version, architecture, and vanilla feature flags).
- The asset index and deduplicated asset objects.
- Required Windows native JARs and their extracted files, honoring exclusions.
- The client logging configuration when declared.
- Legacy virtual assets and per-version resources layouts when the index requests them.

The installer handles both old classifier-based native metadata and modern
standalone native artifacts. Native architecture follows the launcher build
(x64, x86, or ARM64), not a guessed Java installation. Old versions without ARM64
natives return an error on an ARM64 launcher; use an x64 build/runtime for them.
Installation commands currently support Windows only. Java is not needed to
install files; launching and runtime compatibility selection come later.

### Storage

Game files live under **`%LOCALAPPDATA%\com.ember.launcher\game`** on Windows,
using Tauri's application-local data directory. Nothing is downloaded into the
Git checkout or the user's existing `.minecraft` directory.

| Directory | Contents |
| --- | --- |
| `versions/<id>/` | Verified version JSON, client JAR, legacy per-version resources |
| `libraries/` | Shared Maven library JARs and native archives |
| `assets/indexes/` | Verified asset indexes |
| `assets/objects/` | Shared content-addressed asset objects |
| `assets/virtual/` | Legacy asset layouts |
| `assets/log_configs/` | Official client logging configuration |
| `natives/<id>/` | Extracted natives for that version |
| `launcher/installations/` | Completion/error records and verified file hashes |
| `launcher/partial/` | Temporary downloads and staged native extraction |
| `launcher/install.lock` | Cross-process installer lock |

The Installations page shows version ID, status, storage location, and a
**Repair / reverify** button. Startup, recheck, and Play verify installed files
on disk using hashes, so checking a large library can take some time. Play becomes
available after installation, but only reports readiness. It never starts Minecraft.

### Verification, repair, and interrupted work

Files are streamed to temporary files, checked against Mojang's SHA-1 and size
where supplied, and only then published. Files that already verify are reused.
Repair downloads missing/corrupt files and rebuilds the native directory from
verified archives. It does not redownload healthy files. Every published file,
including extracted natives, is recorded with a hash for installed-state checks.
The installation is marked complete only after all files and extraction succeed.

Progress shows stage, current file, file counts, per-file bytes, and overall
completion. Overall percent reserves the final portion for legacy layouts,
native extraction, and saving completion; it is not a bandwidth estimate.

Only one installation/repair runs at a time, including across app processes, to
protect shared libraries/assets as well as prevent duplicate version installs.
Cancel keeps completed verified downloads. An in-flight stalled network request
can take up to its 60-second timeout to stop. Partial files are not reused; the
next installation clears stale staging and resumes by verifying completed files.
Network errors, hash mismatches, permission failures, and disk/write errors appear
in the UI. A file-size disk-space check runs before downloads, and extraction/write
errors are also reported. Retry with Install or Repair after correcting the cause.
Interrupted/crashed installs are shown as incomplete, never ready.

Installation and repair can reuse files offline when both manifests and all
required downloads are already cached. Missing files still need a network
connection. No authentication, Fabric, Forge, mods, cosmetics, or Minecraft
process launching are implemented.

## Version cache and Java Settings

The release list comes from
`https://piston-meta.mojang.com/mc/game/version_manifest_v2.json`. It displays only
stable releases and labels Mojang's `latest.release`. Refresh preserves selection.
The last validated manifest is cached at
`%LOCALAPPDATA%\com.ember.launcher\version-manifest.json`; network failures fall
back to that cache with a warning. Without a usable cache, an error and retry
control are shown. Browser-only mode cannot call the native commands.

Settings retains Java discovery from JAVA_HOME, absolute PATH entries, common
Windows vendor folders, and `%USERPROFILE%\.jdks`. It displays major/full version,
vendor, and executable path. Probes use `-XshowSettings:properties -version`, a
three-second timeout, and no console window. Obvious PATH shims are resolved
through `java.home` where a real executable exists; distinct runtimes are kept.
Display paths remove `\\?\` prefixes while preserving UNC paths. Manual Java
selection is not implemented; `java::inspect(path)` remains reusable for it later.

## Windows checklist before merging PR #3

1. Close the launcher. Fetch and switch to `codex/vanilla-installer`, then run the
   verification commands above and `npm run tauri:dev`.
2. Install a small/older release first, such as **1.6.4**, then a modern release
   such as **1.20.1**. Watch progress. Check the folders above, the ready state,
   native files, and version/status/location on Installations.
3. Click Play after installation. It must only report readiness, with no game
   process. Run Repair and confirm healthy client/library files keep their
   modification times (native files are intentionally rebuilt).
4. Close the launcher, delete that version's client JAR, and reopen. It should
   show repair needed. Repair must restore the JAR. Repeat by replacing a library
   JAR's contents, then an extracted DLL; each should be detected and repaired.
5. Cancel an installation partway through, then resume. Completed verified files
   should be reused. Try double-clicking Install and opening another launcher
   instance: concurrent installs should be rejected cleanly.
6. Optional offline check: temporarily block only the launcher in Windows Firewall
   rather than disconnecting Ethernet. Restart and confirm the cached selector and
   installed list work. A missing-file repair should fail clearly and retry after
   unblocking. The fixture tests cover offline reuse without a PC-wide network change.
7. Open Settings, check Java paths and duplicate cleanup, then run
   `npm run tauri:build` and repeat a basic install/repair in the packaged app.

Windows native runtime, real disk-full/permission conditions, packaged-app IPC,
and a complete official game installation require manual validation. Automated
tests use tiny local HTTP fixtures and official version JSON snapshots; they do
not download a complete game or launch it.

This project is not affiliated with Mojang Studios or Microsoft.

### Windows platform detection

Installation reads the native kernel version through `ntdll!RtlGetVersion` using
Windows SDK types from `windows-sys`. It does not spawn a shell, parse localized
text, or depend on `SystemRoot`. Windows 11 retains its actual `10.0.<build>`
version for Mojang rule matching. The launcher process architecture still maps
x64 to `amd64`, x86 to `x86`, and ARM64 to `aarch64`, including emulated x64 apps
on ARM64 Windows. Detection failures report the failed API or NTSTATUS rather
than silently guessing a version. Windows CI runs a live detection smoke test
as well as platform-independent formatting and architecture tests.
