import { useState } from "react";
import { setToken } from "./lib/auth";

export function Login({ onAuthed }: { onAuthed: () => void }) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const r = await fetch("/api/status", {
        headers: { Authorization: `Bearer ${draft}` },
      });
      if (r.status === 401) {
        setError("Token rejected");
        return;
      }
      if (!r.ok) {
        setError(`HTTP ${r.status}`);
        return;
      }
      setToken(draft);
      onAuthed();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="login">
      <h1>larkstack console</h1>
      <form onSubmit={submit}>
        <label>
          Token
          <input
            type="password"
            autoFocus
            autoComplete="current-password"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="CONSOLE_TOKEN value"
          />
        </label>
        <button type="submit" disabled={busy || !draft}>
          {busy ? "Checking…" : "Sign in"}
        </button>
        {error && <p className="error">{error}</p>}
      </form>
    </main>
  );
}
