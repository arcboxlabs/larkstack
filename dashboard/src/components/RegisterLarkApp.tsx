import { Button } from "@base-ui/react/button";
import { Field } from "@base-ui/react/field";
import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { mutate } from "swr";
import useSWRMutation from "swr/mutation";
import { errMessage, mutateRequest } from "../lib/http";

export interface LarkAppRow {
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

interface Props {
  /// When set, the form edits an existing entry (name readonly, prefilled).
  editing?: LarkAppRow | null;
  /// Called after a successful save with the saved entry name.
  onSaved?: (name: string) => void;
  /// Called when the user abandons an in-progress edit.
  onCancelEdit?: () => void;
}

/// The "register a Lark app" form card: name + credentials, a dry-run **Test**
/// and a **Save** that live-tests server-side before persisting. Shared by the
/// Lark Apps tab (register + edit) and the first-run Setup screen (register).
/// On success it revalidates `/api/lark-apps` so every consumer refreshes.
export function RegisterLarkApp({ editing, onSaved, onCancelEdit }: Props) {
  const {
    register,
    handleSubmit,
    reset,
    getValues,
    formState: { errors },
  } = useForm<LarkAppForm>({ defaultValues: EMPTY });
  const [feedback, setFeedback] = useState<Feedback>(null);

  // Re-seed the form whenever the edit target changes (or clears).
  useEffect(() => {
    reset(
      editing
        ? {
            name: editing.name,
            app_id: editing.app_id,
            app_secret: "",
            base_url: editing.base_url,
          }
        : EMPTY,
    );
    setFeedback(null);
  }, [editing, reset]);

  const save = useSWRMutation(
    "/api/lark-apps",
    (url: string, { arg }: { arg: LarkAppForm }) =>
      mutateRequest(url, { json: { name: arg.name.trim(), ...creds(arg) } }),
    { onSuccess: () => mutate("/api/lark-apps") },
  );
  // Dry-run: answers 200 `{ ok:false }` on bad creds, so it is read, not thrown.
  const test = useSWRMutation(
    "/api/lark-apps/test",
    (url: string, { arg }: { arg: LarkAppForm }) =>
      mutateRequest<TestResult>(url, { json: creds(arg) }),
    { revalidate: false, populateCache: false },
  );

  const onSave = handleSubmit(async (form) => {
    setFeedback(null);
    try {
      await save.trigger(form);
      const name = form.name.trim();
      setFeedback({ tone: "ok", text: `saved "${name}"` });
      if (!editing) reset(EMPTY);
      onSaved?.(name);
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    }
  });

  // Test needs only app_id + app_secret, so it reads values directly rather than
  // going through `handleSubmit` (which would also require `name`).
  const onTest = async () => {
    setFeedback(null);
    const form = getValues();
    if (!form.app_id.trim() || !form.app_secret) {
      setFeedback({
        tone: "error",
        text: "app_id and app_secret are required",
      });
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

  const busy = save.isMutating || test.isMutating;

  return (
    <div className="action-card">
      <div className="actions-subsystem">
        {editing ? `update "${editing.name}"` : "register a Lark app"}
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
            readOnly={!!editing}
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
        {editing && onCancelEdit && (
          <Button type="button" onClick={onCancelEdit} disabled={busy}>
            Cancel
          </Button>
        )}
        {feedback && (
          <span className={`action-result ${feedback.tone}`}>
            {feedback.text}
          </span>
        )}
      </div>
    </div>
  );
}
