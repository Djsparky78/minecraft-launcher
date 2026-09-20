import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Box, ChevronDown, Download, Gamepad2, Settings, UserRound } from "lucide-react";

interface VersionList { versions: { id: string; url: string }[]; cached: boolean; warning: string | null }
interface JavaInstallation { path: string; version: string }

function App() {
  const [version, setVersion] = useState("");
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
      setVersion(current => result.versions.some(v => v.id === current) ? current : result.versions[0]?.id ?? "");
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
  const [status, setStatus] = useState("Ready to play");
  const [launching, setLaunching] = useState(false);

  async function handlePlay() {
    setLaunching(true);
    setStatus("Checking launcher services…");

    try {
      const message = await invoke<string>("launcher_status");
      setStatus(message);
    } catch {
      setStatus("Launcher service unavailable");
    } finally {
      window.setTimeout(() => setLaunching(false), 650);
    }
  }

  return (
    <main className="shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><Box size={21} strokeWidth={2.4} /></div>
          <div><strong>EMBER</strong><span>LAUNCHER</span></div>
        </div>

        <nav aria-label="Launcher navigation">
          <button className="nav-item active"><Gamepad2 size={19} /> Play</button>
          <button className="nav-item"><Download size={19} /> Installations</button>
          <button className="nav-item"><Settings size={19} /> Settings</button>
        </nav>

        <div className="account">
          <div className="avatar"><UserRound size={19} /></div>
          <div><strong>Guest player</strong><span>Offline mode</span></div>
        </div>
      </aside>

      <section className="content">
        <header>
          <div>
            <p className="eyebrow">MINECRAFT: JAVA EDITION</p>
            <h1>Your next world<br />starts here.</h1>
          </div>
          <div className="status-pill"><i /> {status}</div>
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
              <select id="version" disabled={loading || !versions.length} value={version} onChange={(event) => setVersion(event.target.value)}>
                {!versions.length && <option value="">{loading ? "Loading releases…" : "No versions available"}</option>}
                {versions.map((item) => <option key={item.id} value={item.id}>{item.id}</option>)}
              </select>
              <ChevronDown size={18} />
            </div>
          </div>
          <button className="play-button" onClick={handlePlay} disabled={launching || loading || !version}>
            {launching ? "PREPARING…" : "PLAY"}
          </button>
        </section>

        <section className="environment" aria-label="Launcher environment">
          <p role="status">{loading ? "Loading Minecraft releases…" : versionMessage}</p>
          <button onClick={() => void loadVersions()} disabled={loading}>Refresh versions</button>
          <h3>Detected Java installations</h3>
          <div aria-live="polite">
            {javaLoading ? <p>Detecting Java…</p> : javaError ? <p role="alert">{javaError}</p> : java.length ?
              <ul>{java.map(item => <li key={item.path}><strong>Java {item.version}</strong><code>{item.path}</code></li>)}</ul> :
              <p>No Java found. Install Java or set JAVA_HOME, then scan again.</p>}
          </div>
          <button onClick={() => void scanJava()} disabled={javaLoading}>Scan for Java</button>
          <p>Play checks launcher services only. Java compatibility and game launching come later.</p>
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
