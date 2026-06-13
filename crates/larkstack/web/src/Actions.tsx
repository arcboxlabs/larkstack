import { useState } from "react";
import { api } from "./auth";

interface ActionParam {
  name: string;
  label: string;
  required?: boolean;
  placeholder?: string;
}

interface Action {
  name: string;
  description: string;
  params?: ActionParam[];
}

const CATALOG: Record<string, Action[]> = {
  "linear-bridge": [
    { name: "ping", description: "Emit a pong log event (smoke test the action plumbing)" },
    { name: "test-lark", description: "Post a test message to the configured Lark webhook" },
  ],
  "standup-bot": [
    { name: "announce", description: "Ensure tomorrow's doc and post the announcement card",
      params: [{ name: "date", label: "date (today | tomorrow | YYYY-MM-DD)", placeholder: "tomorrow" }] },
    { name: "ensure", description: "Create tomorrow's doc + share with chat (no card)",
      params: [{ name: "date", label: "date", placeholder: "tomorrow" }] },
    { name: "remind", description: "DM everyone still empty for today's doc",
      params: [{ name: "date", label: "date", placeholder: "today" }] },
    { name: "urgent", description: "Remind + in-app urgent escalation for today's doc",
      params: [{ name: "date", label: "date", placeholder: "today" }] },
    { name: "check", description: "List missing fillers for today (read-only)",
      params: [{ name: "date", label: "date", placeholder: "today" }] },
    { name: "urgent-user", description: "Escalate one specific user (for testing)",
      params: [
        { name: "open_id", label: "open_id", required: true, placeholder: "ou_xxx" },
        { name: "date", label: "date", placeholder: "today" },
      ] },
  ],
  "meeting-digest": [
    { name: "process-meeting", description: "Backfill / re-process one meeting by ID",
      params: [
        { name: "meeting_id", label: "meeting_id", required: true, placeholder: "VC meeting ID" },
        { name: "owner", label: "owner (optional override)", placeholder: "open_id" },
        { name: "url", label: "url (skip VC lookup, use this URL)", placeholder: "https://…" },
      ] },
  ],
};

type Result =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "ok"; message: string }
  | { kind: "error"; message: string };

export function Actions() {
  const [results, setResults] = useState<Record<string, Result>>({});
  const [forms, setForms] = useState<Record<string, Record<string, string>>>({});

  const setFormField = (key: string, name: string, value: string) =>
    setForms((f) => ({ ...f, [key]: { ...(f[key] ?? {}), [name]: value } }));

  const invoke = async (subsystem: string, action: Action) => {
    const key = `${subsystem}/${action.name}`;
    const params = forms[key] ?? {};
    const missing = (action.params ?? []).filter(
      (p) => p.required && !params[p.name]?.trim(),
    );
    if (missing.length) {
      setResults((r) => ({
        ...r,
        [key]: { kind: "error", message: `missing: ${missing.map((p) => p.name).join(", ")}` },
      }));
      return;
    }
    // Strip empty optional fields so JSON only carries actual values.
    const body: Record<string, string> = {};
    for (const [k, v] of Object.entries(params)) {
      if (v?.trim()) body[k] = v.trim();
    }
    setResults((r) => ({ ...r, [key]: { kind: "running" } }));
    try {
      const r = await api(`/api/actions/${subsystem}/${action.name}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(Object.keys(body).length ? body : null),
      });
      const j = (await r.json().catch(() => ({}))) as { ok?: boolean; error?: string };
      if (!r.ok) {
        setResults((rs) => ({
          ...rs,
          [key]: { kind: "error", message: j.error ?? `HTTP ${r.status}` },
        }));
      } else {
        setResults((rs) => ({
          ...rs,
          [key]: { kind: "ok", message: "dispatched" },
        }));
        window.setTimeout(
          () => setResults((rs) => ({ ...rs, [key]: { kind: "idle" } })),
          2500,
        );
      }
    } catch (e) {
      setResults((rs) => ({
        ...rs,
        [key]: { kind: "error", message: String(e) },
      }));
    }
  };

  return (
    <section>
      <h2>Actions</h2>
      <p className="muted help-text">
        Dispatch is fire-and-forget. The outcome of each action shows up in the{" "}
        <a href="#events">Events</a> tab.
      </p>
      {Object.entries(CATALOG).map(([subsystem, actions]) => (
        <div key={subsystem} className="actions-group">
          <div className="actions-subsystem">{subsystem}</div>
          {actions.length === 0 ? (
            <div className="muted help-text">no actions defined yet</div>
          ) : (
            <div className="action-cards">
              {actions.map((a) => {
                const key = `${subsystem}/${a.name}`;
                const r = results[key] ?? { kind: "idle" };
                const params = forms[key] ?? {};
                return (
                  <div key={a.name} className={`action-card ${r.kind}`}>
                    <div className="action-card-head">
                      <div>
                        <code className="action-name">{a.name}</code>
                        <div className="muted help-text">{a.description}</div>
                      </div>
                      <button
                        className={`action-btn ${r.kind}`}
                        onClick={() => invoke(subsystem, a)}
                        disabled={r.kind === "running"}
                      >
                        {r.kind === "running" ? "…" : "Run"}
                      </button>
                    </div>
                    {a.params && (
                      <div className="action-fields">
                        {a.params.map((p) => (
                          <label key={p.name}>
                            <span>
                              {p.label}
                              {p.required && <span className="req"> *</span>}
                            </span>
                            <input
                              type="text"
                              value={params[p.name] ?? ""}
                              placeholder={p.placeholder}
                              onChange={(e) =>
                                setFormField(key, p.name, e.target.value)
                              }
                            />
                          </label>
                        ))}
                      </div>
                    )}
                    {r.kind === "error" && (
                      <div className="action-result error">{r.message}</div>
                    )}
                    {r.kind === "ok" && (
                      <div className="action-result ok">{r.message}</div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ))}
    </section>
  );
}
