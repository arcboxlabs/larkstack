import { Field } from "@base-ui/react/field";
import { useState } from "react";
import { Link } from "react-router";
import useSWR, { mutate } from "swr";
import { errMessage, mutateRequest } from "../lib/http";
import type { LarkAppRow } from "./RegisterLarkApp";
import { Select } from "./Select";

type Feedback = { tone: "ok" | "error"; text: string } | null;

interface AppManifest {
  name: string;
  lark_app?: string;
}

interface AppsResponse {
  apps: AppManifest[];
}

/// Bind (or clear) the `[lark-apps.<name>]` this app uses for Lark credentials,
/// from the UI rather than by hand-editing TOML. Writes `[<app>].lark_app` via
/// `PUT /api/config/{app}/lark-app`; the supervisor restarts the app on the
/// resulting config broadcast, so the new binding takes effect with no manual
/// restart. Delivery (bot DMs / group cards) needs a bound app, so this sits at
/// the top of each app's page.
export function LarkBinding({ appName }: { appName: string }) {
  const { data: appsData } = useSWR<AppsResponse>("/api/apps");
  const { data: registry } = useSWR<{ lark_apps: LarkAppRow[] }>(
    "/api/lark-apps",
  );
  const [pending, setPending] = useState(false);
  const [feedback, setFeedback] = useState<Feedback>(null);

  const apps = registry?.lark_apps ?? [];
  const current =
    appsData?.apps.find((a) => a.name === appName)?.lark_app ?? "";

  const onChange = async (next: string) => {
    setPending(true);
    setFeedback(null);
    try {
      await mutateRequest(
        `/api/config/${encodeURIComponent(appName)}/lark-app`,
        {
          method: "PUT",
          json: { lark_app: next || null },
        },
      );
      // The binding lives in /api/apps; the rebind restarts the app, so refresh
      // status too.
      await Promise.all([mutate("/api/apps"), mutate("/api/status")]);
      setFeedback({
        tone: "ok",
        text: next ? `bound to "${next}"` : "unbound",
      });
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="action-card binding-card">
      <div className="actions-subsystem">Lark app</div>
      <p className="muted help-text">
        The Lark credentials this app delivers with. Manage the registry in the{" "}
        <Link to="/lark-apps">Lark Apps</Link> tab.
      </p>
      <Field.Root className="field">
        <Field.Label className="field-label" htmlFor={`lark-app-${appName}`}>
          bound app
        </Field.Label>
        <Select
          id={`lark-app-${appName}`}
          className="field-input field-select"
          value={current}
          disabled={pending || apps.length === 0}
          onValueChange={onChange}
          options={[
            { value: "", label: "— none (use env / inline) —" },
            ...apps.map((a) => ({ value: a.name, label: a.name })),
          ]}
        />
      </Field.Root>
      {apps.length === 0 && (
        <p className="muted help-text">
          No Lark apps registered yet — add one in the{" "}
          <Link to="/lark-apps">Lark Apps</Link> tab first.
        </p>
      )}
      {feedback && (
        <span className={`action-result ${feedback.tone}`}>
          {feedback.text}
        </span>
      )}
    </div>
  );
}
