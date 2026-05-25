import { useMemo, useState } from "react";
import { useEvents, type Level } from "./useEvents";

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

export function Events() {
  const { events, connected, laggedCount } = useEvents();
  const [minLevel, setMinLevel] = useState<Level>("info");
  const [subsystem, setSubsystem] = useState<string>("");

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
          <label>
            level{" "}
            <select
              value={minLevel}
              onChange={(e) => setMinLevel(e.target.value as Level)}
            >
              <option value="trace">trace+</option>
              <option value="debug">debug+</option>
              <option value="info">info+</option>
              <option value="warn">warn+</option>
              <option value="error">error</option>
            </select>
          </label>
          <label>
            subsystem{" "}
            <select
              value={subsystem}
              onChange={(e) => setSubsystem(e.target.value)}
            >
              <option value="">all</option>
              {subsystems.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          </label>
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
            <span
              className="level"
              style={{ color: LEVEL_COLORS[e.level] }}
            >
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
