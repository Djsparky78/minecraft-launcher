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
