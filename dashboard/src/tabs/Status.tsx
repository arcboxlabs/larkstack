import { Switch } from "@base-ui/react/switch";
import { useState } from "react";
import { Link } from "react-router";
import useSWR, { mutate } from "swr";
import { errMessage, mutateRequest } from "../lib/http";

/// Apps that have a dedicated config page. Their card title links there; apps
/// without one (e.g. preview-only integrations) render a plain, static title.
const APP_ROUTES: Record<string, string> = {
  linear: "/linear",
  github: "/github",
  gitlab: "/gitlab",
  standup: "/standup",
};

type State = "starting" | "running" | "errored" | "stopped";

interface Subsystem {
  state: State;
  message: string | null;
  updated_at: number;
}

interface StatusResponse {
  subsystems: Record<string, Subsystem>;
}

interface AppManifest {
  name: string;
  kind: "integration" | "automation";
  description: string;
  enabled: boolean;
}

interface AppsResponse {
  apps: AppManifest[];
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
  const { data: status, error } = useSWR<StatusResponse>("/api/status", {
    refreshInterval: 3000,
  });
  const { data: appsData } = useSWR<AppsResponse>("/api/apps");
  const subsystems = status?.subsystems ?? {};
  const apps = appsData?.apps;

  const [pending, setPending] = useState<string | null>(null);
  const [toggleError, setToggleError] = useState<string | null>(null);

  const onToggle = async (name: string, enabled: boolean) => {
    setPending(name);
    setToggleError(null);
    try {
      await mutateRequest(`/api/config/${encodeURIComponent(name)}/enabled`, {
        method: "PUT",
        json: { enabled },
      });
      await Promise.all([mutate("/api/apps"), mutate("/api/status")]);
    } catch (e) {
      setToggleError(`${name}: ${errMessage(e)}`);
    } finally {
      setPending(null);
    }
  };

  return (
    <section>
      <h2>Apps</h2>
      {error && <p className="error">Failed to load: {errMessage(error)}</p>}
      {toggleError && <p className="error">{toggleError}</p>}
      {!apps && <p>Loading…</p>}
      {apps && apps.length === 0 && <p className="muted">no apps registered</p>}
      {apps && apps.length > 0 && (
        <div className="status-grid">
          {apps.map((app) => {
            const s = subsystems[app.name];
            const state: State =
              s?.state ?? (app.enabled ? "starting" : "stopped");
            const route = APP_ROUTES[app.name];
            return (
              <article key={app.name} className={`status-card ${state}`}>
                <header>
                  {route ? (
                    <Link className="status-name link" to={route}>
                      {app.name}
                    </Link>
                  ) : (
                    <span className="status-name">{app.name}</span>
                  )}
                  <div className="status-controls">
                    <span
                      className="status-pill"
                      style={{
                        color: STATE_COLORS[state],
                        borderColor: STATE_COLORS[state],
                      }}
                    >
                      {state}
                    </span>
                    <Switch.Root
                      className="switch"
                      checked={app.enabled}
                      disabled={pending === app.name}
                      onCheckedChange={(checked) => onToggle(app.name, checked)}
                      aria-label={`${app.enabled ? "disable" : "enable"} ${app.name}`}
                    >
                      <Switch.Thumb className="switch-thumb" />
                    </Switch.Root>
                  </div>
                </header>
                {s?.message && <p className="status-msg">{s.message}</p>}
                <footer className="muted">
                  {s ? `updated ${freshness(s.updated_at)}` : "not started"}
                </footer>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
