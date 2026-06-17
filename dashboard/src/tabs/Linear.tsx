import { AlertDialog } from "@base-ui/react/alert-dialog";
import { Button } from "@base-ui/react/button";
import { Field } from "@base-ui/react/field";
import { useEffect, useState } from "react";
import { type Control, Controller, useForm } from "react-hook-form";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import { Checkbox } from "../components/Checkbox";
import { LarkBinding } from "../components/LarkBinding";
import { type EventOption, RoutingEditor } from "../components/RoutingEditor";
import { Select } from "../components/Select";
import { errMessage, mutateRequest } from "../lib/http";

type Feedback = { tone: "ok" | "error"; text: string } | null;

// Linear routes by team key (the identifier prefix, e.g. `ENG`); a `*` rule or a
// default destination catches everything.
const LINEAR_EVENTS: EventOption[] = [
  { value: "issue", label: "Issues (create / update)" },
  { value: "comment", label: "Comments" },
];

// ── Settings ───────────────────────────────────────────────────────────────

interface SettingsWire {
  subscriber_on_comment: boolean;
  subscriber_on_status_change: boolean;
  subscriber_on_any_update: boolean;
  reminders_enabled: boolean;
  reminder_recipients: string;
  reminder_lead_days: number[];
  reminder_overdue_max_days: number;
  reminder_check_interval_hours: number;
  reminder_timezone: string;
}

// The form mirrors the wire shape but holds lead-days as an editable CSV string.
interface SettingsForm extends Omit<SettingsWire, "reminder_lead_days"> {
  reminder_lead_days: string;
}

const DEFAULT_FORM: SettingsForm = {
  subscriber_on_comment: true,
  subscriber_on_status_change: true,
  subscriber_on_any_update: false,
  reminders_enabled: true,
  reminder_recipients: "assignee",
  reminder_lead_days: "7, 3, 1, 0",
  reminder_overdue_max_days: 7,
  reminder_check_interval_hours: 6,
  reminder_timezone: "UTC",
};

function wireToForm(w: SettingsWire): SettingsForm {
  return { ...w, reminder_lead_days: w.reminder_lead_days.join(", ") };
}

function formToWire(f: SettingsForm): SettingsWire {
  return {
    subscriber_on_comment: f.subscriber_on_comment,
    subscriber_on_status_change: f.subscriber_on_status_change,
    subscriber_on_any_update: f.subscriber_on_any_update,
    reminders_enabled: f.reminders_enabled,
    reminder_recipients: f.reminder_recipients,
    reminder_lead_days: f.reminder_lead_days
      .split(",")
      .map((s) => parseInt(s.trim(), 10))
      .filter((n) => Number.isFinite(n) && n >= 0),
    reminder_overdue_max_days: f.reminder_overdue_max_days,
    reminder_check_interval_hours: f.reminder_check_interval_hours,
    reminder_timezone: f.reminder_timezone.trim(),
  };
}

// Boolean fields of the settings form — the ones rendered as checkboxes.
type BoolField =
  | "subscriber_on_comment"
  | "subscriber_on_status_change"
  | "subscriber_on_any_update"
  | "reminders_enabled";

function CheckboxField({
  label,
  name,
  control,
}: {
  label: string;
  name: BoolField;
  control: Control<SettingsForm>;
}) {
  return (
    <Field.Root className="field">
      <Field.Label className="field-label">{label}</Field.Label>
      <Controller
        control={control}
        name={name}
        render={({ field }) => (
          <Checkbox
            checked={!!field.value}
            onCheckedChange={field.onChange}
            inputRef={field.ref}
            name={field.name}
          />
        )}
      />
    </Field.Root>
  );
}

function SettingsCard() {
  const { data, error, mutate } = useSWR<SettingsWire>(
    "/api/apps/linear/settings",
  );
  const { register, handleSubmit, reset, control } = useForm<SettingsForm>({
    defaultValues: DEFAULT_FORM,
  });
  const [feedback, setFeedback] = useState<Feedback>(null);

  // Hydrate the form once settings load.
  useEffect(() => {
    if (data) reset(wireToForm(data));
  }, [data, reset]);

  const save = useSWRMutation(
    "/api/apps/linear/settings",
    (url: string, { arg }: { arg: SettingsWire }) =>
      mutateRequest<SettingsWire>(url, { method: "PUT", json: arg }),
    {
      onSuccess: (saved) => {
        if (saved) void mutate(saved, { revalidate: false });
      },
    },
  );

  const onSave = handleSubmit(async (form) => {
    setFeedback(null);
    try {
      await save.trigger(formToWire(form));
      setFeedback({ tone: "ok", text: "settings saved" });
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    }
  });

  return (
    <div className="action-card">
      <div className="actions-subsystem">behavior settings</div>
      {error && <p className="error">Failed to load: {errMessage(error)}</p>}

      <p className="muted help-text">
        Subscriber fan-out &amp; due-date reminders need{" "}
        <code>LINEAR_API_KEY</code> set (to resolve subscriber emails / poll due
        dates). Changes apply live — no restart.
      </p>

      <div className="action-fields">
        <CheckboxField
          label="Notify subscribers on comments"
          name="subscriber_on_comment"
          control={control}
        />
        <CheckboxField
          label="Notify subscribers on status changes"
          name="subscriber_on_status_change"
          control={control}
        />
        <CheckboxField
          label="Notify subscribers on any field update"
          name="subscriber_on_any_update"
          control={control}
        />
        <CheckboxField
          label="Enable due-date reminders"
          name="reminders_enabled"
          control={control}
        />

        <Field.Root className="field">
          <Field.Label className="field-label">Reminder recipients</Field.Label>
          <Controller
            control={control}
            name="reminder_recipients"
            render={({ field }) => (
              <Select
                className="field-input field-select"
                value={field.value}
                onValueChange={field.onChange}
                options={[
                  { value: "assignee", label: "Assignee only" },
                  {
                    value: "assignee_and_subscribers",
                    label: "Assignee + all subscribers",
                  },
                ]}
              />
            )}
          />
        </Field.Root>

        <Field.Root className="field">
          <Field.Label className="field-label">Reminder lead days</Field.Label>
          <Field.Control
            className="field-input"
            placeholder="7, 3, 1, 0"
            {...register("reminder_lead_days")}
          />
        </Field.Root>

        <Field.Root className="field">
          <Field.Label className="field-label">
            Overdue reminders cap (days)
          </Field.Label>
          <Field.Control
            className="field-input"
            type="number"
            min={0}
            {...register("reminder_overdue_max_days", { valueAsNumber: true })}
          />
        </Field.Root>

        <Field.Root className="field">
          <Field.Label className="field-label">
            Check interval (hours)
          </Field.Label>
          <Field.Control
            className="field-input"
            type="number"
            min={1}
            {...register("reminder_check_interval_hours", {
              valueAsNumber: true,
            })}
          />
        </Field.Root>

        <Field.Root className="field">
          <Field.Label className="field-label">Timezone (IANA)</Field.Label>
          <Field.Control
            className="field-input"
            placeholder="UTC"
            {...register("reminder_timezone")}
          />
        </Field.Root>
      </div>

      <div className="filters" style={{ marginTop: "0.75rem" }}>
        <Button type="button" onClick={onSave} disabled={save.isMutating}>
          {save.isMutating ? "Saving…" : "Save settings"}
        </Button>
        {feedback && (
          <span className={`action-result ${feedback.tone}`}>
            {feedback.text}
          </span>
        )}
      </div>
    </div>
  );
}

// ── User map ─────────────────────────────────────────────────────────────────

interface Mapping {
  linear_email: string;
  lark_email: string;
  lark_open_id: string | null;
  note: string | null;
  updated_by: string | null;
  updated_at: number;
}

interface MappingForm {
  linear_email: string;
  lark_email: string;
  note: string;
}

const EMPTY_MAPPING: MappingForm = {
  linear_email: "",
  lark_email: "",
  note: "",
};

function UserMapCard() {
  const { data, error, mutate } = useSWR<Mapping[]>(
    "/api/apps/linear/user-map",
  );
  const rows = data;

  const {
    register,
    handleSubmit,
    reset,
    formState: { errors },
  } = useForm<MappingForm>({ defaultValues: EMPTY_MAPPING });
  const [feedback, setFeedback] = useState<Feedback>(null);
  const [target, setTarget] = useState<string | null>(null);

  const save = useSWRMutation(
    "/api/apps/linear/user-map",
    (url: string, { arg }: { arg: MappingForm }) =>
      mutateRequest(url, {
        json: {
          linear_email: arg.linear_email.trim(),
          lark_email: arg.lark_email.trim(),
          note: arg.note.trim() || null,
        },
      }),
    { onSuccess: () => mutate() },
  );
  const remove = useSWRMutation(
    "/api/apps/linear/user-map",
    (_url: string, { arg }: { arg: string }) =>
      mutateRequest(`/api/apps/linear/user-map/${encodeURIComponent(arg)}`, {
        method: "DELETE",
      }),
    { onSuccess: () => mutate() },
  );

  const onSave = handleSubmit(async (form) => {
    setFeedback(null);
    try {
      await save.trigger(form);
      setFeedback({ tone: "ok", text: `mapped ${form.linear_email.trim()}` });
      reset(EMPTY_MAPPING);
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    }
  });

  const confirmDelete = async () => {
    if (!target) return;
    setFeedback(null);
    try {
      await remove.trigger(target);
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    } finally {
      setTarget(null);
    }
  };

  return (
    <div className="action-card" style={{ marginTop: "1.5rem" }}>
      <div className="actions-subsystem">user map (Linear → Lark email)</div>
      <p className="muted help-text">
        Override the DM target when a person's Linear and Lark emails differ.
        When they match, no entry is needed.
      </p>

      <div className="action-fields">
        <Field.Root className="field" invalid={!!errors.linear_email}>
          <Field.Label className="field-label">
            linear_email<span className="req"> *</span>
          </Field.Label>
          <Field.Control
            className="field-input"
            placeholder="alice@linear.example"
            {...register("linear_email", {
              required: "linear_email is required",
            })}
          />
          {errors.linear_email && (
            <Field.Error className="field-error" match>
              {errors.linear_email.message}
            </Field.Error>
          )}
        </Field.Root>
        <Field.Root className="field" invalid={!!errors.lark_email}>
          <Field.Label className="field-label">
            lark_email<span className="req"> *</span>
          </Field.Label>
          <Field.Control
            className="field-input"
            placeholder="alice@lark.example"
            {...register("lark_email", { required: "lark_email is required" })}
          />
          {errors.lark_email && (
            <Field.Error className="field-error" match>
              {errors.lark_email.message}
            </Field.Error>
          )}
        </Field.Root>
        <Field.Root className="field">
          <Field.Label className="field-label">note</Field.Label>
          <Field.Control
            className="field-input"
            placeholder="optional"
            {...register("note")}
          />
        </Field.Root>
      </div>

      <div className="filters" style={{ marginTop: "0.75rem" }}>
        <Button type="button" onClick={onSave} disabled={save.isMutating}>
          {save.isMutating ? "Saving…" : "Save mapping"}
        </Button>
        {feedback && (
          <span className={`action-result ${feedback.tone}`}>
            {feedback.text}
          </span>
        )}
      </div>

      {error && <p className="error">Failed to load: {errMessage(error)}</p>}
      {rows && rows.length > 0 && (
        <table style={{ marginTop: "1.5rem" }}>
          <thead>
            <tr>
              <th>linear_email</th>
              <th>lark_email</th>
              <th>note</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((m) => (
              <tr key={m.linear_email}>
                <td>
                  <code>{m.linear_email}</code>
                </td>
                <td>
                  <code>{m.lark_email}</code>
                </td>
                <td className="muted">{m.note ?? ""}</td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <Button
                    className="action-btn error"
                    onClick={() => setTarget(m.linear_email)}
                  >
                    Delete
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {rows && rows.length === 0 && (
        <p className="muted help-text">No overrides yet.</p>
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
              Delete mapping for "{target}"?
            </AlertDialog.Title>
            <AlertDialog.Description className="dialog-desc">
              DMs will fall back to the Linear email. This cannot be undone.
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
    </div>
  );
}

export function Linear() {
  return (
    <section>
      <h2>Linear</h2>
      <LarkBinding appName="linear" />
      <RoutingEditor
        appName="linear"
        eventOptions={LINEAR_EVENTS}
        showUserMap={false}
        showAlertLabels={false}
      />
      <SettingsCard />
      <UserMapCard />
    </section>
  );
}
