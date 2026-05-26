import { useState } from "react";

interface Action {
  name: string;
  description: string;
}

const CATALOG: Record<string, Action[]> = {
  "linear-bridge": [
    { name: "ping", description: "Emit a pong log event (smoke test the action plumbing)" },
    { name: "test-lark", description: "Post a test message to the configured Lark webhook" },
  ],
  "standup-bot": [
    { name: "announce", description: "Ensure tomorrow's doc and post the announcement card" },
    { name: "ensure", description: "Create tomorrow's doc + share with chat (no card)" },
    { name: "remind", description: "DM everyone still empty for today's doc" },
    { name: "urgent", description: "Remind + in-app urgent escalation for today's doc" },
    { name: "check", description: "List missing fillers for today (read-only)" },
  ],
  "meeting-digest": [
    // process-meeting needs params (meeting_id); reachable via curl, not button.
  ],
};

type Result =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "ok"; message: string }
  | { kind: "error"; message: string };

export function Actions() {
  const [results, setResults] = useState<Record<string, Result>>({});

  const invoke = async (subsystem: string, action: string) => {
    const key = `${subsystem}/${action}`;
    setResults((r) => ({ ...r, [key]: { kind: "running" } }));
    try {
      const r = await fetch(`/api/actions/${subsystem}/${action}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "null",
      });
      const body = (await r.json().catch(() => ({}))) as {
        ok?: boolean;
        error?: string;
      };
      if (!r.ok) {
        setResults((rs) => ({
          ...rs,
          [key]: { kind: "error", message: body.error ?? `HTTP ${r.status}` },
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
      {Object.entries(CATALOG).map(([subsystem, actions]) => (
        <div key={subsystem} className="actions-group">
          <div className="actions-subsystem">{subsystem}</div>
          {actions.length === 0 ? (
            <div className="muted" style={{ fontSize: "0.8rem" }}>
              no parameterless actions — see <code>/api/actions/{subsystem}/&lt;name&gt;</code>
            </div>
          ) : (
          <div className="actions-row">
            {actions.map((a) => {
              const key = `${subsystem}/${a.name}`;
              const r = results[key] ?? { kind: "idle" };
              return (
                <button
                  key={a.name}
                  title={a.description}
                  onClick={() => invoke(subsystem, a.name)}
                  disabled={r.kind === "running"}
                  className={`action-btn ${r.kind}`}
                >
                  {a.name}
                  {r.kind === "running" && " …"}
                  {r.kind === "ok" && " ✓"}
                  {r.kind === "error" && ` ! ${r.message}`}
                </button>
              );
            })}
          </div>
          )}
        </div>
      ))}
    </section>
  );
}
