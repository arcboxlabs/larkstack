import { AlertDialog } from "@base-ui/react/alert-dialog";
import { Button } from "@base-ui/react/button";
import { useState } from "react";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import {
  type LarkAppRow,
  RegisterLarkApp,
} from "../components/RegisterLarkApp";
import { errMessage, mutateRequest } from "../lib/http";

export function LarkApps() {
  const { data, error, mutate } = useSWR<{ lark_apps: LarkAppRow[] }>(
    "/api/lark-apps",
  );
  const apps = data?.lark_apps;

  const [editing, setEditing] = useState<LarkAppRow | null>(null);
  const [target, setTarget] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);

  const remove = useSWRMutation(
    "/api/lark-apps",
    (_url: string, { arg }: { arg: string }) =>
      mutateRequest(`/api/lark-apps/${encodeURIComponent(arg)}`, {
        method: "DELETE",
      }),
    { onSuccess: () => mutate() },
  );

  const confirmDelete = async () => {
    if (!target) return;
    setFeedback(null);
    try {
      await remove.trigger(target);
      setTarget(null);
    } catch (e) {
      setTarget(null);
      setFeedback(errMessage(e));
    }
  };

  return (
    <section>
      <h2>Lark Apps</h2>
      <p className="muted help-text">
        Credentials are shared here and referenced from an app's config with{" "}
        <code>lark_app = "&lt;name&gt;"</code>. Saving live-tests the
        credentials against Lark and only persists if they work.
      </p>

      <RegisterLarkApp
        editing={editing}
        onSaved={() => setEditing(null)}
        onCancelEdit={() => setEditing(null)}
      />

      {error && <p className="error">Failed to load: {errMessage(error)}</p>}
      {feedback && <p className="error">{feedback}</p>}
      {apps && apps.length > 0 && (
        <table style={{ marginTop: "1.5rem" }}>
          <thead>
            <tr>
              <th>name</th>
              <th>app_id</th>
              <th>base_url</th>
              <th>secret</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {apps.map((a) => (
              <tr key={a.name}>
                <td>
                  <code>{a.name}</code>
                </td>
                <td>
                  <code>{a.app_id}</code>
                </td>
                <td className="muted">{a.base_url}</td>
                <td>
                  {a.has_secret ? (
                    "set"
                  ) : (
                    <span className="error">missing</span>
                  )}
                </td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <Button className="action-btn" onClick={() => setEditing(a)}>
                    Edit
                  </Button>{" "}
                  <Button
                    className="action-btn error"
                    onClick={() => setTarget(a.name)}
                  >
                    Delete
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {apps && apps.length === 0 && (
        <p className="muted help-text">No Lark apps registered yet.</p>
      )}

      <AlertDialog.Root
        open={target !== null}
        onOpenChange={(open) => {
          if (!open) setTarget(null);
        }}
      >
        <AlertDialog.Portal>
          <AlertDialog.Backdrop className="dialog-backdrop" />
          <AlertDialog.Popup className="dialog-popup">
            <AlertDialog.Title className="dialog-title">
              Delete lark-app "{target}"?
            </AlertDialog.Title>
            <AlertDialog.Description className="dialog-desc">
              Apps bound to it will error. This cannot be undone.
            </AlertDialog.Description>
            <div className="dialog-actions">
              <AlertDialog.Close
                className="action-btn"
                disabled={remove.isMutating}
              >
                Cancel
              </AlertDialog.Close>
              <Button
                className="action-btn error"
                type="button"
                onClick={confirmDelete}
                disabled={remove.isMutating}
              >
                {remove.isMutating ? "Deleting…" : "Delete"}
              </Button>
            </div>
          </AlertDialog.Popup>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </section>
  );
}
