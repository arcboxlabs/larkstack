import { Button } from "@base-ui/react/button";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import {
  type LarkAppRow,
  RegisterLarkApp,
} from "../components/RegisterLarkApp";
import { errMessage, mutateRequest } from "../lib/http";

interface ConsoleAuth {
  configured: boolean;
  lark_app?: string;
  admins: string[];
  redirect_uri?: string;
  scope?: string;
}

/// Split an admin list pasted as comma / whitespace / newline separated.
function parseAdmins(raw: string): string[] {
  return raw
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

/// First-run guided setup: register a Lark app, then bind it as the console's
/// sign-in client. Binding is the moment the console stops being open — this
/// screen makes that explicit and hands the operator straight to the login flow.
export function Setup() {
  const { data: registry } = useSWR<{ lark_apps: LarkAppRow[] }>(
    "/api/lark-apps",
  );
  const { data: current } = useSWR<ConsoleAuth>("/api/console-auth");
  const apps = registry?.lark_apps ?? [];
  const hasApp = apps.length > 0;

  const callbackUrl = `${window.location.origin}/auth/callback`;

  const [larkApp, setLarkApp] = useState("");
  const [admins, setAdmins] = useState("");
  const [error, setError] = useState<string | null>(null);

  // Prefill from any existing binding, and default the dropdown to the first
  // registered app once the registry loads.
  useEffect(() => {
    if (current?.lark_app) setLarkApp(current.lark_app);
    else if (apps[0]) setLarkApp((v) => v || apps[0].name);
    if (current?.admins?.length) setAdmins(current.admins.join(", "));
  }, [current, apps]);

  const bind = useSWRMutation(
    "/api/console-auth",
    (url: string, { arg }: { arg: ConsoleAuth }) =>
      mutateRequest(url, { method: "PUT", json: arg }),
  );

  const onSecure = async () => {
    setError(null);
    if (!larkApp) {
      setError("Choose a Lark app to sign in with.");
      return;
    }
    try {
      await bind.trigger({
        configured: true,
        lark_app: larkApp,
        admins: parseAdmins(admins),
      });
      // Binding now enforces sign-in; hand off to the OAuth flow.
      window.location.assign("/auth/login");
    } catch (e) {
      setError(errMessage(e));
    }
  };

  return (
    <section className="setup">
      <h2>Secure your console</h2>
      <div className="banner-warn" style={{ marginBottom: "1.5rem" }}>
        ⚠ This console is <strong>open</strong> — anyone who can reach it has
        full admin access. Bind a Lark app below to require sign-in.
      </div>

      <ol className="setup-steps">
        <li>
          <div className="setup-step-head">
            <span className="setup-step-num">1</span>
            <span className="setup-step-title">Register a Lark app</span>
            {hasApp && <span className="setup-done">✓ done</span>}
          </div>
          <p className="muted help-text">
            Create a custom app in the{" "}
            <a
              href="https://open.larksuite.com/app"
              target="_blank"
              rel="noreferrer"
            >
              Lark Developer Console
            </a>
            , then add its credentials here. Saving live-tests them against
            Lark.
          </p>
          {hasApp ? (
            <p className="muted help-text">
              Registered: {apps.map((a) => a.name).join(", ")}. Add or manage
              more in the <Link to="/lark-apps">Lark Apps</Link> tab.
            </p>
          ) : (
            <RegisterLarkApp onSaved={(n) => setLarkApp(n)} />
          )}
        </li>

        <li>
          <div className="setup-step-head">
            <span className={`setup-step-num ${hasApp ? "" : "muted"}`}>2</span>
            <span className="setup-step-title">Bind console sign-in</span>
          </div>
          <p className="muted help-text">
            In your Lark app's security settings, register this redirect URI and
            grant the user-info permission:
          </p>
          <code className="setup-callback">{callbackUrl}</code>

          <div className="action-fields" style={{ marginTop: "1rem" }}>
            <div className="field">
              <label className="field-label" htmlFor="setup-lark-app">
                sign in with
              </label>
              <select
                id="setup-lark-app"
                className="field-input"
                value={larkApp}
                onChange={(e) => setLarkApp(e.target.value)}
                disabled={!hasApp}
              >
                {!hasApp && <option value="">register an app first</option>}
                {apps.map((a) => (
                  <option key={a.name} value={a.name}>
                    {a.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label className="field-label" htmlFor="setup-admins">
                admin emails
              </label>
              <textarea
                id="setup-admins"
                className="field-input setup-admins"
                placeholder="you@example.com, teammate@example.com"
                value={admins}
                onChange={(e) => setAdmins(e.target.value)}
                disabled={!hasApp}
              />
            </div>
          </div>
          <p className="muted help-text">
            Only these Lark accounts may sign in. <strong>Leave empty</strong>{" "}
            to allow any user in your tenant.
          </p>
          <p className="muted help-text">
            After saving, sign-in is required immediately — make sure you can
            sign in with one of the emails above (or an empty list). If you get
            locked out, clear <code>[console].lark_app</code> in{" "}
            <code>config.toml</code> on the server to reopen it.
          </p>

          <div className="filters" style={{ marginTop: "0.5rem" }}>
            <Button
              type="button"
              onClick={onSecure}
              disabled={!hasApp || bind.isMutating}
            >
              {bind.isMutating ? "Securing…" : "Secure & sign in"}
            </Button>
            <Link className="action-btn" to="/status">
              Skip for now
            </Link>
            {error && <span className="action-result error">{error}</span>}
          </div>
        </li>
      </ol>
    </section>
  );
}
