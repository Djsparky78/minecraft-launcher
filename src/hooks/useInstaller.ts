import { useCallback, useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";

export interface Installation { version: string; status: string; location: string; error: string | null }
export interface InstallProgress {
  version: string; stage: string; file: string;
  completed_files: number; total_files: number; percent: number;
  file_bytes: number; file_total: number | null;
}
export function useInstaller() {
  const [installations, setInstallations] = useState<Installation[]>([]);
  const [checking, setChecking] = useState(true);
  const [listError, setListError] = useState("");
  const [activeVersion, setActiveVersion] = useState("");
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [cancelling, setCancelling] = useState(false);
  const busy = useRef(false);
  const cancelled = useRef(false);
  const generation = useRef(0);

  const refresh = useCallback(async () => {
    const request = ++generation.current;
    setChecking(true); setListError("");
    try {
      const list = await invoke<Installation[]>("installed_versions");
      if (request === generation.current) setInstallations(list);
      return list;
    } catch (e) {
      if (request === generation.current) setListError(String(e));
      throw e;
    } finally { if (request === generation.current) setChecking(false); }
  }, []);
  useEffect(() => { void refresh().catch(() => {}); }, [refresh]);

  async function install(version: string, repair = false) {
    if (busy.current || !version) return;
    busy.current = true; cancelled.current = false;
    setActiveVersion(version); setCancelling(false); setProgress(null); setError("");
    setMessage(`Checking ${version}…`);
    try {
      // Recheck disk state before treating Play as ready, even after external edits.
      const list = await refresh();
      if (cancelled.current) { setMessage("Installation cancelled before download started."); return; }
      if (!repair && list.some(item => item.version === version && item.status === "installed")) {
        setMessage(`${version} is installed and ready. Game launching is not available yet.`);
        return;
      }
      setMessage(`${repair ? "Repairing" : "Installing"} ${version}…`);
      const channel = new Channel<InstallProgress>();
      channel.onmessage = event => { if (event.version === version) setProgress(event); };
      await invoke("install_version", { version, onProgress: channel });
      setMessage(`${version} installed and ready. Game launching is not available yet.`);
    } catch (e) { setError(String(e)); setMessage(`Installation of ${version} did not complete.`); }
    finally {
      await refresh().catch(() => {});
      busy.current = false; setActiveVersion(""); setCancelling(false);
    }
  }
  async function cancel() {
    if (!activeVersion) return;
    cancelled.current = true; setCancelling(true);
    try { await invoke("cancel_install", { version: activeVersion }); }
    catch (e) { setError(String(e)); setCancelling(false); }
  }
  return { installations, checking, listError, activeVersion, progress, message, error, cancelling, refresh, install, cancel };
}
