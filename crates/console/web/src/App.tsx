import { useEffect, useState } from "react";
import { Config } from "./Config";
import { Events } from "./Events";

type State = "starting" | "running" | "errored" | "stopped";

interface Status {
  state: State;
  message: string | null;
  updated_at: number;
}

interface StatusResponse {
  subsystems: Record<string, Status>;
}

const STATE_COLORS: Record<State, string> = {
  starting: "#888",
  running: "#22c55e",
  errored: "#ef4444",
  stopped: "#6b7280",
};

function StatusTable() {
  const [data, setData] = useState<StatusResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const r = await fetch("/api/status");
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        setData(await r.json());
        setError(null);
      } catch (e) {
        setError(String(e));
      }
    };
    fetchStatus();
    const id = setInterval(fetchStatus, 3000);
    return () => clearInterval(id);
  }, []);

  return (
    <section>
      <h2>Subsystems</h2>
      {error && <p className="error">Failed to load: {error}</p>}
      {!data && !error && <p>Loading…</p>}
      {data && (
        <table>
          <thead>
            <tr>
              <th>Subsystem</th>
              <th>State</th>
              <th>Message</th>
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {Object.entries(data.subsystems).length === 0 && (
              <tr>
                <td colSpan={4} className="muted">
                  no subsystems reporting yet
                </td>
              </tr>
            )}
            {Object.entries(data.subsystems).map(([name, s]) => (
              <tr key={name}>
                <td>{name}</td>
                <td>
                  <span
                    className="dot"
                    style={{ background: STATE_COLORS[s.state] }}
                  />
                  {s.state}
                </td>
                <td className="muted">{s.message ?? "—"}</td>
                <td className="muted">
                  {new Date(s.updated_at).toLocaleTimeString()}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

export function App() {
  return (
    <main>
      <h1>larkstack console</h1>
      <StatusTable />
      <Config />
      <Events />
    </main>
  );
}
