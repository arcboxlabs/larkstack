import { useEffect, useState } from "react";
import { api } from "./auth";

interface LarkAppRow {
  name: string;
  app_id: string;
  base_url: string;
  has_secret: boolean;
}

interface FormState {
  name: string;
  app_id: string;
  app_secret: string;
  base_url: string;
}

type Status =
  | { kind: "idle" }
  | { kind: "testing" }
  | { kind: "saving" }
  | { kind: "ok"; message: string }
  | { kind: "error"; message: string };

const EMPTY: FormState = { name: "", app_id: "", app_secret: "", base_url: "" };
const DEFAULT_BASE = "https://open.larksuite.com";

export function LarkApps() {
  const [apps, setApps] = useState<LarkAppRow[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  const load = () => {
    api("/api/lark-apps")
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((j: { lark_apps: LarkAppRow[] }) => {
        setApps(j.lark_apps);
        setLoadError(null);
      })
      .catch((e) => setLoadError(String(e)));
  };

  useEffect(load, []);

  const set = (k: keyof FormState, v: string) =>
    setForm((f) => ({ ...f, [k]: v }));

  const body = () => ({
    app_id: form.app_id.trim(),
    app_secret: form.app_secret,
    base_url: form.base_url.trim() || DEFAULT_BASE,
  });

  const onTest = async () => {
    if (!form.app_id.trim() || !form.app_secret) {
      setStatus({ kind: "error", message: "app_id and app_secret are required" });
      return;
    }
    setStatus({ kind: "testing" });
    try {
      const r = await api("/api/lark-apps/test", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body()),
      });
      const j = (await r.json()) as { ok: boolean; expire?: number; error?: string };
      setStatus(
        j.ok
          ? { kind: "ok", message: `valid — token good for ${j.expire ?? "?"}s` }
          : { kind: "error", message: j.error ?? "credential test failed" },
      );
    } catch (e) {
      setStatus({ kind: "error", message: String(e) });
    }
  };

  const onSave = async () => {
    if (!form.name.trim() || !form.app_id.trim() || !form.app_secret) {
      setStatus({ kind: "error", message: "name, app_id and app_secret are required" });
      return;
    }
    setStatus({ kind: "saving" });
    try {
      const r = await api("/api/lark-apps", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: form.name.trim(), ...body() }),
      });
      const j = (await r.json().catch(() => ({}))) as { error?: string };
      if (!r.ok) {
        setStatus({ kind: "error", message: j.error ?? `HTTP ${r.status}` });
        return;
      }
      setStatus({ kind: "ok", message: `saved "${form.name.trim()}"` });
      setForm(EMPTY);
      load();
    } catch (e) {
      setStatus({ kind: "error", message: String(e) });
    }
  };

  const onDelete = async (name: string) => {
    if (!window.confirm(`Delete lark-app "${name}"? Apps bound to it will error.`)) {
      return;
    }
    try {
      const r = await api(`/api/lark-apps/${encodeURIComponent(name)}`, {
        method: "DELETE",
      });
      if (!r.ok) {
        const j = (await r.json().catch(() => ({}))) as { error?: string };
        setStatus({ kind: "error", message: j.error ?? `HTTP ${r.status}` });
        return;
      }
      load();
    } catch (e) {
      setStatus({ kind: "error", message: String(e) });
    }
  };

  const onEdit = (a: LarkAppRow) => {
    setForm({ name: a.name, app_id: a.app_id, app_secret: "", base_url: a.base_url });
    setStatus({ kind: "idle" });
  };

  const busy = status.kind === "testing" || status.kind === "saving";

  return (
    <section>
      <h2>Lark Apps</h2>
      <p className="muted help-text">
        Credentials are shared here and referenced from an app's config with{" "}
        <code>lark_app = "&lt;name&gt;"</code>. Saving live-tests the credentials
        against Lark and only persists if they work.
      </p>

      <div className="action-card">
        <div className="actions-subsystem">register / update a Lark app</div>
        <div className="action-fields">
          <label>
            <span>
              name<span className="req"> *</span>
            </span>
            <input
              type="text"
              value={form.name}
              placeholder="main"
              onChange={(e) => set("name", e.target.value)}
            />
          </label>
          <label>
            <span>
              app_id<span className="req"> *</span>
            </span>
            <input
              type="text"
              value={form.app_id}
              placeholder="cli_…"
              onChange={(e) => set("app_id", e.target.value)}
            />
          </label>
          <label>
            <span>
              app_secret<span className="req"> *</span>
            </span>
            <input
              type="password"
              value={form.app_secret}
              placeholder="write-only — re-enter to update"
              autoComplete="off"
              onChange={(e) => set("app_secret", e.target.value)}
            />
          </label>
          <label>
            <span>base_url</span>
            <input
              type="text"
              value={form.base_url}
              placeholder={DEFAULT_BASE}
              onChange={(e) => set("base_url", e.target.value)}
            />
          </label>
        </div>
        <div className="filters" style={{ marginTop: "0.75rem" }}>
          <button onClick={onTest} disabled={busy}>
            {status.kind === "testing" ? "Testing…" : "Test"}
          </button>
          <button onClick={onSave} disabled={busy}>
            {status.kind === "saving" ? "Saving…" : "Save"}
          </button>
          {status.kind === "ok" && (
            <span className="action-result ok">{status.message}</span>
          )}
          {status.kind === "error" && (
            <span className="action-result error">{status.message}</span>
          )}
        </div>
      </div>

      {loadError && <p className="error">Failed to load: {loadError}</p>}
      {apps && apps.length > 0 && (
        <table style={{ marginTop: "1.5rem" }}>
          <thead>
            <tr>
              <th>name</th>
              <th>app_id</th>
              <th>base_url</th>
              <th>secret</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {apps.map((a) => (
              <tr key={a.name}>
                <td>
                  <code>{a.name}</code>
                </td>
                <td>
                  <code>{a.app_id}</code>
                </td>
                <td className="muted">{a.base_url}</td>
                <td>{a.has_secret ? "set" : <span className="error">missing</span>}</td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <button className="action-btn" onClick={() => onEdit(a)}>
                    Edit
                  </button>{" "}
                  <button className="action-btn error" onClick={() => onDelete(a.name)}>
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {apps && apps.length === 0 && (
        <p className="muted help-text">No Lark apps registered yet.</p>
      )}
    </section>
  );
}
