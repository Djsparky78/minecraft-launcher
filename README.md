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

## Milestone 4: Microsoft account sign-in (Windows)

Settings now has **Sign in with Microsoft**, **Cancel sign-in**, **Sign out**, and
**Retry saved sign-in**. The sidebar shows the verified Minecraft username after
login; Settings also shows the profile UUID. Installation, repair, versions,
manifest caching, and Java detection work without signing in. Minecraft process
launching is still intentionally unavailable (milestone 5).

### Register your own Microsoft application

You must supply your own application (client) ID. Ember includes no default ID,
client secret, or credentials from another launcher. The client ID is public;
your Microsoft password, authorization code, and tokens are not configuration.

1. Open the [Microsoft Entra admin center](https://entra.microsoft.com/), choose
   the directory where you can register applications, and open **Identity / Entra
   ID > Applications > App registrations > New registration**. If your account
   cannot create app registrations, obtain access to a directory where you have
   that permission. This is a developer setup step, not a Minecraft account issue.
2. Name it **Ember Launcher**. Choose **Personal Microsoft accounts only**. If that
   option is unavailable, **Accounts in any organizational directory and personal
   Microsoft accounts** also supports the personal accounts Ember uses. Do not
   choose an organizational-accounts-only audience.
3. Register the application and copy **Application (client) ID** from Overview.
   Do not use Object ID or Directory (tenant) ID.
4. Open **Authentication > Add a platform > Mobile and desktop applications**.
   Add the custom redirect URI **`http://localhost`** and save. Keep this redirect
   under the native/mobile-and-desktop platform, not Web or SPA. Do not register
   duplicate localhost redirects on different platforms or ports. Ember binds an
   available loopback port before opening your browser, then uses
   `http://localhost:<port>/`; Microsoft ignores the port when matching localhost
   native redirects. No fixed port, public callback server, or DNS setup is needed.
5. Under Authentication's advanced settings, enable **Allow public client flows**
   and save. Leave implicit access-token/ID-token issuance disabled. Do **not**
   create a client secret or certificate for Ember.
6. Ember requests **`XboxLive.signin offline_access`** at sign-in using the
   `consumers` Microsoft identity endpoint. Grant the requested consent in your
   system browser. Ember does not need Microsoft Graph User.Read; Graph alone
   does not provide Xbox/Minecraft access. If your directory restricts consent,
   resolve that policy with its administrator. Do not substitute another app ID
   to work around a registration or consent error.
7. **Minecraft API approval is a separate step.** New application IDs may be
   rejected by Minecraft Services even after Microsoft and Xbox login succeed.
   Submit your application/client ID and accurate app details through the
   [Minecraft application review form](https://aka.ms/mce-reviewappid) and follow
   Microsoft's current requirements. Approval is controlled by Microsoft, not
   Ember. If the form is unavailable or your app is rejected, contact Minecraft
   support; do not borrow another launcher's ID. Ember reports this possibility
   when Minecraft rejects the login exchange.

Reference documentation:
- [Microsoft authorization-code flow and PKCE](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow)
- [Microsoft desktop app configuration](https://learn.microsoft.com/en-us/entra/identity-platform/scenario-desktop-app-configuration)
- [Microsoft localhost redirect rules](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url#localhost-exceptions)
- [Minecraft API approval guidance from minecraft-launcher-lib](https://github.com/JakobDev/minecraft-launcher-lib/blob/master/doc/tutorial/microsoft_login.rst)

### Configure Ember on your Windows PC

For development, open PowerShell in your checkout and set the public client ID
before starting Ember. Replace the placeholder with your actual GUID:

```powershell
cd "$env:USERPROFILE\EmberLauncher"
git fetch origin
git switch codex/microsoft-auth
git pull --ff-only origin codex/microsoft-auth
npm install
$env:EMBER_MICROSOFT_CLIENT_ID = "PASTE-YOUR-APPLICATION-CLIENT-ID-HERE"
npm run tauri:dev
```

The environment variable lasts for this PowerShell session and takes priority
over the config file. A `.env` file is not automatically loaded by the Rust
backend. Never put tokens or passwords in Vite variables.

For persistent configuration, including packaged builds, use
**`%APPDATA%\com.ember.launcher\auth.json`** (Roaming AppData, separate from Local
AppData game downloads). This file contains only the public client ID:

```powershell
$emberConfigDir = Join-Path $env:APPDATA "com.ember.launcher"
New-Item -ItemType Directory -Force $emberConfigDir | Out-Null
$emberConfig = @{ microsoft_client_id = "PASTE-YOUR-APPLICATION-CLIENT-ID-HERE" } | ConvertTo-Json
[System.IO.File]::WriteAllText((Join-Path $emberConfigDir "auth.json"), $emberConfig)
```

Restart Ember after changing the client ID. Sign out before changing IDs; saved
credentials are isolated by client ID. To remove an old ID's credential later,
use Windows **Credential Manager > Windows Credentials > Generic Credentials**
and remove its Ember entry (service `com.ember.launcher.microsoft`). Do not share
credential exports, callback URLs, tokens, or network captures.

### Security and session behavior

- Rust performs Microsoft authorization code + **S256 PKCE**, Xbox Live, XSTS,
  Minecraft login, entitlement/license verification, and profile lookup.
- The normal browser handles Microsoft passwords and MFA. Ember does not embed
  a Microsoft login page or receive a password. No client secret is used.
- The short-lived callback listener binds only IPv4/IPv6 loopback. It checks the
  HTTP method, exact callback path and Host/port, one random state value, and one
  code/error value. Invalid callbacks cannot consume a login. Sign-in times out
  after three minutes; cancellation closes the listener and drops pending work.
- Authentication requests use HTTPS with redirects disabled and bounded
  responses/timeouts. Provider response bodies, codes, and tokens are not logged
  or returned as errors. The webview's CSP/permissions are unchanged.
- Only the Microsoft refresh credential is persisted, using **Windows Credential
  Manager** through `keyring`'s Windows native backend. Access tokens stay in Rust
  memory. No tokens are written to JSON, the repository, or browser storage.
  There is no plain-text fallback; secure-store failures are shown to the user.
- Startup refreshes the saved Microsoft credential and repeats Xbox/Minecraft
  verification. While Ember is open, account checks run every minute; tokens
  within two minutes of expiry are refreshed. Rotated refresh credentials are
  saved before further network requests. Revoked credentials require sign-in.
- Failed account verification never exposes a playable profile. A network failure
  retains the saved credential for retry but does not grant offline account
  access. Downloaded game files and cached versions remain usable.
- Sign-out cancels pending auth, clears in-memory account/token state, and deletes
  the saved refresh credential. It does not clear your browser's Microsoft
  cookies or revoke consent globally. Use Microsoft's account permissions page
  if you also want to revoke the application's grant.
- Authentication supports one account at a time and Windows secure storage in
  this milestone. Other platforms report secure storage unavailable rather than
  storing secrets insecurely. Rust secrets have no Debug/Serialize implementation
  and owned secret buffers are zeroized on drop; HTTP/JSON dependencies may make
  transient in-memory copies. This is not a guarantee against a compromised OS.

### Windows acceptance checklist before merging PR #4

1. Configure your client ID, run the commands above, open **Settings**, and choose
   **Sign in with Microsoft**. Confirm it opens your normal browser. Sign in with
   a personal Microsoft account that has Minecraft Java access and an existing
   Java profile. Finish consent/MFA. Do not paste the callback URL into chat.
2. Return to Ember. Confirm your Minecraft username replaces Guest player and
   Settings shows the correct username and 32-character UUID. It should explicitly
   report verified Java Edition access. Play still must not launch Minecraft.
3. Close Ember, press Ctrl+C in its terminal if needed, and run `npm run tauri:dev`
   again from the same configured shell. Confirm the session restores without a
   browser prompt. This exercises an actual refresh-token exchange on restart.
4. Sign out, restart, and confirm the account does not restore. Sign in again;
   Microsoft may remember you in the browser, which is expected.
5. Start sign-in, then cancel in Ember. Confirm the UI becomes usable. Also try
   declining consent and closing the browser without completing sign-in (wait
   up to three minutes or cancel in Ember). An old callback must not sign you in.
6. If available, test an account without Java access, an account without an Xbox
   profile, and an expired/revoked consent grant. Confirm a useful error, no
   authenticated/playable profile, and the ability to retry or sign out. These
   conditions are covered by mocked tests but need real account acceptance tests.
7. Temporarily block Ember in Windows Firewall, restart, and check the saved-login
   network error. Restore access, choose **Retry saved sign-in**, and verify it
   recovers. Do not delete your saved credential merely to test a network outage.
8. Confirm version selection/cache, Java scanning, Minecraft 1.20.1 installation,
   rechecking, and repair still work. Finally run `npm run tauri:build` and repeat
   sign-in/restart/sign-out using the packaged app and persistent auth.json.

Automated tests use fake credentials/provider responses plus an isolated real
Windows Credential Manager round-trip in CI. They never sign into a real account.
Live Microsoft consent, application approval, account ownership/subscription
variants, MFA/child-account restrictions, browser/firewall behavior, and packaged
interactive sign-in require your Windows acceptance testing.
