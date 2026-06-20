import { Switch } from "@base-ui/react/switch";
import {
  type IconType,
  SiGithub,
  SiGitlab,
  SiLinear,
  SiX,
} from "@icons-pack/react-simple-icons";
import { useState } from "react";
import { Link } from "react-router";
import useSWR, { mutate } from "swr";
import { Spinner } from "../components/Spinner";
import { errMessage, mutateRequest } from "../lib/http";

/// Apps that have a dedicated config page. Their card title links there; any
/// app without an entry here renders a plain, static title.
const APP_ROUTES: Record<string, string> = {
  linear: "/linear",
  github: "/github",
  gitlab: "/gitlab",
  x: "/x",
  standup: "/standup",
  minutes: "/minutes",
};

/// Proper-cased product names + brand logos (simple-icons) for the registered
/// apps. Our own automations have no third-party brand, so they carry only a
/// label and fall back to a monogram tile. Unknown apps render their raw name.
const APP_BRANDS: Record<string, { label: string; Icon?: IconType }> = {
  linear: { label: "Linear", Icon: SiLinear },
  github: { label: "GitHub", Icon: SiGithub },
  gitlab: { label: "GitLab", Icon: SiGitlab },
  x: { label: "X", Icon: SiX },
  standup: { label: "Standup" },
  minutes: { label: "Minutes" },
};

function appLabel(name: string): string {
  return APP_BRANDS[name]?.label ?? name;
}

/// The app's brand logo, or — for our own automations — a neutral monogram tile
/// of the label's first letter. Icons inherit the text color (theme-adaptive).
function AppLogo({ name }: { name: string }) {
  const Icon = APP_BRANDS[name]?.Icon;
  if (Icon) return <Icon className="status-logo" size={18} aria-hidden />;
  return (
    <span className="status-logo status-logo-mono" aria-hidden>
      {appLabel(name).charAt(0).toUpperCase()}
    </span>
  );
}

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
      {!apps && <Spinner />}
      {apps && apps.length === 0 && <p className="muted">no apps registered</p>}
      {apps && apps.length > 0 && (
        <div className="status-grid">
          {apps.map((app) => {
            const s = subsystems[app.name];
            const state: State =
              s?.state ?? (app.enabled ? "starting" : "stopped");
            const route = APP_ROUTES[app.name];
            return (
              <article
                key={app.name}
                className={`status-card ${state}${route ? " linkable" : ""}`}
              >
                {/* Stretched link: the whole card navigates to the app's page,
                    while the controls below sit above this overlay and stay
                    clickable (avoids invalid <button>-inside-<a> nesting). */}
                {route && (
                  <Link
                    className="status-card-link"
                    to={route}
                    aria-label={`open ${appLabel(app.name)} settings`}
                  />
                )}
                <header>
                  <span className="status-name">
                    <AppLogo name={app.name} />
                    {appLabel(app.name)}
                  </span>
                  <div className="status-controls">
                    {/* Pill color comes from the card's state class (CSS) so it
                        stays in the design palette — no inline hex. */}
                    <span className="status-pill">{state}</span>
                    <Switch.Root
                      className="switch"
                      checked={app.enabled}
                      disabled={pending === app.name}
                      onCheckedChange={(checked) => onToggle(app.name, checked)}
                      aria-label={`${app.enabled ? "disable" : "enable"} ${appLabel(app.name)}`}
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
