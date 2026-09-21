import type { useAccount } from "../hooks/useAccount";
export function AccountSettings({ auth }: { auth: ReturnType<typeof useAccount> }) {
  const { account, busy } = auth;
  return <section className="account-settings" aria-label="Microsoft account">
    <h2>Minecraft account</h2>
    {account.profile && <dl><dt>Username</dt><dd>{account.profile.name}</dd><dt>UUID</dt><dd><code>{account.profile.id}</code></dd></dl>}
    <p role={account.status === "error" ? "alert" : "status"}>
      {busy === "sign_in" ? "Complete sign-in in your browser. Ember never sees your Microsoft password." : busy === "restore" ? "Checking saved sign-in…" : busy === "sign_out" ? "Signing out…" : account.message}
    </p>
    <div className="account-actions">
      {account.status !== "signed_in" && <button disabled={!!busy || !account.configured} onClick={() => void auth.signIn()}>Sign in with Microsoft</button>}
      {account.configured && <button disabled={!!busy} onClick={() => void auth.signOut()}>Sign out</button>}
      {account.status === "error" && account.configured && <button disabled={!!busy} onClick={() => void auth.retry()}>Retry saved sign-in</button>}
      {busy && busy !== "sign_out" && <button onClick={() => void auth.cancel()}>Cancel sign-in</button>}
    </div>
    <p className="account-note">Sign-out removes Ember’s saved credentials. It does not sign you out of Microsoft in your browser. Game launching is not available yet.</p>
  </section>;
}
