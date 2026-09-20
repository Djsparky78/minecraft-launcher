import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Box, ChevronDown, Download, Gamepad2, Settings, UserRound } from "lucide-react";

const versions = ["Latest release", "1.21.4", "1.20.1 Forge"];

function App() {
  const [version, setVersion] = useState(versions[0]);
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
              <select id="version" value={version} onChange={(event) => setVersion(event.target.value)}>
                {versions.map((item) => <option key={item}>{item}</option>)}
              </select>
              <ChevronDown size={18} />
            </div>
          </div>
          <button className="play-button" onClick={handlePlay} disabled={launching}>
            {launching ? "PREPARING…" : "PLAY"}
          </button>
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
