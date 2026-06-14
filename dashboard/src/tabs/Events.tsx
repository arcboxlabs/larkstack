import { Select } from "@base-ui/react/select";
import { useLocalStorage } from "foxact/use-local-storage";
import type { ReactNode } from "react";
import { useMemo } from "react";
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

const LEVEL_LABEL: Record<Level, string> = {
  trace: "trace+",
  debug: "debug+",
  info: "info+",
  warn: "warn+",
  error: "error",
};

function FilterSelect({
  value,
  onChange,
  children,
  display,
}: {
  value: string;
  onChange: (v: string) => void;
  children: ReactNode;
  display: (v: string) => string;
}) {
  return (
    <Select.Root
      modal={false}
      value={value}
      onValueChange={(v) => onChange((v as string | null) ?? "")}
    >
      <Select.Trigger className="select-trigger">
        <Select.Value>{(v) => display(v as string)}</Select.Value>
        <Select.Icon className="select-icon">▾</Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner sideOffset={4} align="start">
          <Select.Popup className="select-popup">{children}</Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}

function FilterItem({ value, label }: { value: string; label: string }) {
  return (
    <Select.Item value={value} className="select-item">
      <Select.ItemIndicator className="select-item-indicator">
        ✓
      </Select.ItemIndicator>
      <Select.ItemText>{label}</Select.ItemText>
    </Select.Item>
  );
}

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
            <FilterSelect
              value={minLevel}
              onChange={(v) => setMinLevel((v || "info") as Level)}
              display={(v) => LEVEL_LABEL[v as Level] ?? v}
            >
              {LEVELS.map((l) => (
                <FilterItem key={l.value} value={l.value} label={l.label} />
              ))}
            </FilterSelect>
          </div>
          <div className="filter">
            <span className="filter-label">subsystem</span>
            <FilterSelect
              value={subsystem}
              onChange={setSubsystem}
              display={(v) => v || "all"}
            >
              <FilterItem value="" label="all" />
              {subsystems.map((s) => (
                <FilterItem key={s} value={s} label={s} />
              ))}
            </FilterSelect>
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
