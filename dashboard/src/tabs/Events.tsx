import { useLocalStorage } from "foxact/use-local-storage";
import { useMemo } from "react";
import { Select } from "../components/Select";
import { type Level, useEvents } from "../lib/useEvents";

const LEVEL_ORDER: Record<Level, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
};

const LEVEL_COLORS: Record<Level, string> = {
  trace: "#6b7280",
  debug: "#3b82f6",
  info: "#22c55e",
  warn: "#f59e0b",
  error: "#ef4444",
};

const LEVELS: ReadonlyArray<{ value: Level; label: string }> = [
  { value: "trace", label: "trace+" },
  { value: "debug", label: "debug+" },
  { value: "info", label: "info+" },
  { value: "warn", label: "warn+" },
  { value: "error", label: "error" },
];

export function Events() {
  const { events, connected, laggedCount } = useEvents();
  // Filters persist across reloads (foxact localStorage state, raw strings).
  const [minLevel, setMinLevel] = useLocalStorage<Level>(
    "larkstack.events.level",
    "info",
    { raw: true },
  );
  const [subsystem, setSubsystem] = useLocalStorage<string>(
    "larkstack.events.subsystem",
    "",
    { raw: true },
  );

  const subsystems = useMemo(() => {
    const set = new Set<string>();
    for (const e of events) {
      if (e.subsystem) set.add(e.subsystem);
    }
    return Array.from(set).sort();
  }, [events]);

  const filtered = useMemo(() => {
    const min = LEVEL_ORDER[minLevel];
    return events
      .filter((e) => LEVEL_ORDER[e.level] >= min)
      .filter((e) => !subsystem || e.subsystem === subsystem)
      .slice()
      .reverse();
  }, [events, minLevel, subsystem]);

  return (
    <section>
      <header className="events-header">
        <h2>Events</h2>
        <div className="filters">
          <div className="filter">
            <span className="filter-label">level</span>
            <Select
              value={minLevel}
              onValueChange={(v) => setMinLevel((v || "info") as Level)}
              options={LEVELS}
            />
          </div>
          <div className="filter">
            <span className="filter-label">subsystem</span>
            <Select
              value={subsystem}
              onValueChange={setSubsystem}
              options={[
                { value: "", label: "all" },
                ...subsystems.map((s) => ({ value: s, label: s })),
              ]}
            />
          </div>
          <span className={`conn ${connected ? "ok" : "down"}`}>
            {connected ? "● live" : "○ reconnecting"}
          </span>
          {laggedCount > 0 && (
            <span className="lag">dropped {laggedCount}</span>
          )}
        </div>
      </header>
      <ul className="log">
        {filtered.length === 0 && <li className="muted">no events yet</li>}
        {filtered.map((e) => (
          <li key={e.id}>
            <span className="ts muted">
              {new Date(e.timestamp).toLocaleTimeString()}
            </span>
            <span className="level" style={{ color: LEVEL_COLORS[e.level] }}>
              {e.level.toUpperCase()}
            </span>
            {e.subsystem && <span className="subsys">[{e.subsystem}]</span>}
            <span className="msg">{e.message}</span>
            {Object.keys(e.fields).length > 0 && (
              <span className="fields muted">
                {Object.entries(e.fields)
                  .map(([k, v]) => `${k}=${JSON.stringify(v)}`)
                  .join(" ")}
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
