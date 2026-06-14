import useSWR from "swr";
import { errMessage } from "../lib/http";

type State = "starting" | "running" | "errored" | "stopped";

interface Subsystem {
  state: State;
  message: string | null;
  updated_at: number;
}

interface StatusResponse {
  subsystems: Record<string, Subsystem>;
}

const STATE_COLORS: Record<State, string> = {
  starting: "#888",
  running: "#22c55e",
  errored: "#ef4444",
  stopped: "#6b7280",
};

function freshness(ms: number): string {
  const dt = Date.now() - ms;
  if (dt < 1000) return "just now";
  if (dt < 60_000) return `${Math.floor(dt / 1000)}s ago`;
  if (dt < 3_600_000) return `${Math.floor(dt / 60_000)}m ago`;
  return `${Math.floor(dt / 3_600_000)}h ago`;
}

export function Status() {
  const { data, error, isLoading } = useSWR<StatusResponse>("/api/status", {
    refreshInterval: 3000,
  });

  return (
    <section>
      <h2>Subsystems</h2>
      {error && <p className="error">Failed to load: {errMessage(error)}</p>}
      {isLoading && <p>Loading…</p>}
      {data && (
        <div className="status-grid">
          {Object.entries(data.subsystems).length === 0 && (
            <p className="muted">no subsystems reporting yet</p>
          )}
          {Object.entries(data.subsystems).map(([name, s]) => (
            <article key={name} className={`status-card ${s.state}`}>
              <header>
                <span className="status-name">{name}</span>
                <span
                  className="status-pill"
                  style={{
                    color: STATE_COLORS[s.state],
                    borderColor: STATE_COLORS[s.state],
                  }}
                >
                  {s.state}
                </span>
              </header>
              {s.message && <p className="status-msg">{s.message}</p>}
              <footer className="muted">updated {freshness(s.updated_at)}</footer>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
