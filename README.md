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
npm test
npm run build
cd src-tauri
cargo check
cd ..
npm run tauri:build
```

The test suite verifies version selection and confirms that the Play button invokes the `launcher_status` Rust command.

## Current scope

This first milestone establishes the application shell. Microsoft authentication, Minecraft metadata, game installation, Java management, and process launching will be added in later milestones.

This project is not affiliated with Mojang Studios or Microsoft.

## Minecraft releases and Java discovery

The desktop app fetches the official Mojang manifest at
`https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` and lists only
`release` entries in Mojang's newest-first order. Snapshots, Forge and placeholder
versions are excluded. Refresh preserves your selection when it still exists.
Requests time out after 15 seconds. A validated manifest is saved atomically to
Tauri's app cache directory (`%LOCALAPPDATA%\com.ember.launcher\version-manifest.json`
on Windows). If Mojang is unavailable, the saved releases appear with a warning.
A missing or corrupt cache produces a retryable error. A cache write failure does
not prevent using newly fetched releases.

Java detection runs independently and returns all working installations found via
`JAVA_HOME`, absolute PATH entries, common Java vendor directories under Program
Files / Program Files (x86) / LocalAppData, and `%USERPROFILE%\.jdks`. It probes
`java.exe -version` without opening console windows, deduplicates resolved paths,
and skips failed probes or probes exceeding three seconds. Both legacy Java 8
and modern OpenJDK version strings are supported. Arbitrary custom locations
must be added to PATH or JAVA_HOME; registry-only installations and Minecraft's
bundled runtimes are not exhaustively searched. Java compatibility with the
selected Minecraft release is not evaluated yet.

The `versions` and `java` Rust modules contain the discovery logic; thin Tauri
commands expose typed results to React. Browser-only mode cannot call these
commands. Play still calls only the existing status command and starts no game.

### Windows acceptance checklist

1. Run `npm install`, `npm test`, `npm run build`, and `npx tsc --noEmit`.
2. Run `cargo check --locked --manifest-path src-tauri/Cargo.toml` and
   `cargo test --locked --manifest-path src-tauri/Cargo.toml`.
3. Run `npm run tauri:dev`. Confirm real releases load and you can change selection.
4. Refresh and confirm the selection stays. Snapshots and Forge must not appear.
5. After one successful refresh, disconnect the network and refresh again. Within
   15 seconds, saved versions should appear with an offline warning. Restart the
   desktop app offline to verify the cache survives restarts.
6. Close the app and temporarily rename the cache file, then reopen offline.
   Confirm a useful error and disabled Play button. Reconnect and refresh to recover.
   Repeat with invalid JSON in the cache to check corrupt-cache recovery.
7. Confirm Java versions and full executable paths match each installation's
   `java.exe -version`. Test PATH, JAVA_HOME, a common vendor folder, and multiple
   installations. Scan again should not duplicate a resolved executable.
8. Test with no discoverable Java (or a clean Windows account). Confirm the no-Java
   message, and that scanning remains usable after installing/configuring Java.
9. Press Play and confirm only `Launcher services ready` appears. No game starts.
10. Run `npm run tauri:build` and repeat the online/offline and Java checks in the
    packaged app. Verify long Java paths remain readable and the window scrolls.
