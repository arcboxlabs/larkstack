import { useState } from "react";
import { AlertDialog } from "@base-ui/react/alert-dialog";
import { Button } from "@base-ui/react/button";
import { Field } from "@base-ui/react/field";
import { useForm } from "react-hook-form";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import { errMessage, mutateRequest } from "../lib/http";

interface LarkAppRow {
  name: string;
  app_id: string;
  base_url: string;
  has_secret: boolean;
}

interface LarkAppForm {
  name: string;
  app_id: string;
  app_secret: string;
  base_url: string;
}

interface TestResult {
  ok: boolean;
  expire?: number;
  error?: string;
}

type Feedback = { tone: "ok" | "error"; text: string } | null;

const DEFAULT_BASE = "https://open.larksuite.com";
const EMPTY: LarkAppForm = {
  name: "",
  app_id: "",
  app_secret: "",
  base_url: DEFAULT_BASE,
};

/// Credentials portion (no name) — shared by Save and the dry-run Test.
function creds(form: LarkAppForm) {
  return {
    app_id: form.app_id.trim(),
    app_secret: form.app_secret,
    base_url: form.base_url.trim() || DEFAULT_BASE,
  };
}

export function LarkApps() {
  const { data, error, mutate } = useSWR<{ lark_apps: LarkAppRow[] }>(
    "/api/lark-apps",
  );
  const apps = data?.lark_apps;

  const {
    register,
    handleSubmit,
    reset,
    getValues,
    formState: { errors },
  } = useForm<LarkAppForm>({ defaultValues: EMPTY });
  const [editing, setEditing] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<Feedback>(null);
  const [target, setTarget] = useState<string | null>(null);

  // POST live-tests the credentials server-side then upserts; revalidate the list.
  const save = useSWRMutation(
    "/api/lark-apps",
    (url: string, { arg }: { arg: LarkAppForm }) =>
      mutateRequest(url, { json: { name: arg.name.trim(), ...creds(arg) } }),
    { onSuccess: () => mutate() },
  );
  // Dry-run: answers 200 `{ ok:false }` on bad creds, so it is read, not thrown.
  const test = useSWRMutation(
    "/api/lark-apps/test",
    (url: string, { arg }: { arg: LarkAppForm }) =>
      mutateRequest<TestResult>(url, { json: creds(arg) }),
    { revalidate: false, populateCache: false },
  );
  const remove = useSWRMutation(
    "/api/lark-apps",
    (_url: string, { arg }: { arg: string }) =>
      mutateRequest(`/api/lark-apps/${encodeURIComponent(arg)}`, {
        method: "DELETE",
      }),
    { onSuccess: () => mutate() },
  );

  const onSave = handleSubmit(async (form) => {
    setFeedback(null);
    try {
      await save.trigger(form);
      setFeedback({ tone: "ok", text: `saved "${form.name.trim()}"` });
      reset(EMPTY);
      setEditing(null);
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    }
  });

  // Test needs only app_id + app_secret, so it reads the values directly rather
  // than going through `handleSubmit` (which would also require `name`).
  const onTest = async () => {
    setFeedback(null);
    const form = getValues();
    if (!form.app_id.trim() || !form.app_secret) {
      setFeedback({ tone: "error", text: "app_id and app_secret are required" });
      return;
    }
    try {
      const r = await test.trigger(form);
      setFeedback(
        r?.ok
          ? { tone: "ok", text: `valid — token good for ${r.expire ?? "?"}s` }
          : { tone: "error", text: r?.error ?? "credential test failed" },
      );
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    }
  };

  const confirmDelete = async () => {
    if (!target) return;
    setFeedback(null);
    try {
      await remove.trigger(target);
      setTarget(null);
    } catch (e) {
      setTarget(null);
      setFeedback({ tone: "error", text: errMessage(e) });
    }
  };

  const onEdit = (a: LarkAppRow) => {
    reset({ name: a.name, app_id: a.app_id, app_secret: "", base_url: a.base_url });
    setEditing(a.name);
    setFeedback(null);
  };

  const busy = save.isMutating || test.isMutating;

  return (
    <section>
      <h2>Lark Apps</h2>
      <p className="muted help-text">
        Credentials are shared here and referenced from an app's config with{" "}
        <code>lark_app = "&lt;name&gt;"</code>. Saving live-tests the credentials
        against Lark and only persists if they work.
      </p>

      <div className="action-card">
        <div className="actions-subsystem">
          {editing ? `update "${editing}"` : "register a Lark app"}
        </div>
        <div className="action-fields">
          <Field.Root className="field" invalid={!!errors.name}>
            <Field.Label className="field-label">
              name<span className="req"> *</span>
            </Field.Label>
            <Field.Control
              className="field-input"
              placeholder="main"
              {...register("name", { required: "name is required" })}
              readOnly={editing !== null}
            />
            {errors.name && (
              <Field.Error className="field-error" match>
                {errors.name.message}
              </Field.Error>
            )}
          </Field.Root>
          <Field.Root className="field" invalid={!!errors.app_id}>
            <Field.Label className="field-label">
              app_id<span className="req"> *</span>
            </Field.Label>
            <Field.Control
              className="field-input"
              placeholder="cli_…"
              {...register("app_id", { required: "app_id is required" })}
            />
            {errors.app_id && (
              <Field.Error className="field-error" match>
                {errors.app_id.message}
              </Field.Error>
            )}
          </Field.Root>
          <Field.Root className="field" invalid={!!errors.app_secret}>
            <Field.Label className="field-label">
              app_secret<span className="req"> *</span>
            </Field.Label>
            <Field.Control
              className="field-input"
              type="password"
              autoComplete="off"
              placeholder="write-only — re-enter to update"
              {...register("app_secret", { required: "app_secret is required" })}
            />
            {errors.app_secret && (
              <Field.Error className="field-error" match>
                {errors.app_secret.message}
              </Field.Error>
            )}
          </Field.Root>
          <Field.Root className="field">
            <Field.Label className="field-label">base_url</Field.Label>
            <Field.Control
              className="field-input"
              placeholder={DEFAULT_BASE}
              {...register("base_url")}
            />
          </Field.Root>
        </div>
        <div className="filters" style={{ marginTop: "0.75rem" }}>
          <Button type="button" onClick={onTest} disabled={busy}>
            {test.isMutating ? "Testing…" : "Test"}
          </Button>
          <Button type="button" onClick={onSave} disabled={busy}>
            {save.isMutating ? "Saving…" : editing ? "Update" : "Save"}
          </Button>
          {feedback && (
            <span className={`action-result ${feedback.tone}`}>
              {feedback.text}
            </span>
          )}
        </div>
      </div>

      {error && <p className="error">Failed to load: {errMessage(error)}</p>}
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
                  {a.has_secret ? "set" : <span className="error">missing</span>}
                </td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <Button className="action-btn" onClick={() => onEdit(a)}>
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
