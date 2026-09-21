import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface AccountView {
  status: "signed_out" | "signed_in" | "error";
  configured: boolean;
  profile: { name: string; id: string } | null;
  message: string;
}
const initial: AccountView = { status: "signed_out", configured: false, profile: null, message: "Checking saved sign-in…" };
function errorMessage(error: unknown): string {
  // Backend exposes a sanitized message, never raw provider response bodies.
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") return error.message;
  return "Could not contact the account service. Restart Ember and try again.";
}
export function useAccount() {
  const [account, setAccount] = useState<AccountView>(initial);
  const [busy, setBusy] = useState<"restore" | "sign_in" | "sign_out" | null>("restore");
  const running = useRef(false);
  const mounted = useRef(true);
  const run = useCallback(async (operation: "restore" | "sign_in" | "sign_out") => {
    if (running.current) return;
    running.current = true;
    setBusy(operation);
    try {
      const result = await invoke<AccountView>(`auth_${operation}`);
      if (mounted.current) setAccount(result);
    } catch (error) {
      if (mounted.current) setAccount(current => ({...current, status: "error", profile: null, message: errorMessage(error)}));
    } finally {
      running.current = false;
      if (mounted.current) setBusy(null);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void run("restore");
    const timer = window.setInterval(() => { void run("restore"); }, 60_000);
    return () => { mounted.current = false; window.clearInterval(timer); };
  }, [run]);
  const cancel = async () => {
    try { await invoke("auth_cancel"); }
    catch (error) { setAccount(current => ({...current, message: errorMessage(error)})); }
  };
  return { account, busy, signIn: () => run("sign_in"), signOut: () => run("sign_out"), retry: () => run("restore"), cancel };
}
