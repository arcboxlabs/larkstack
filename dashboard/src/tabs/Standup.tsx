import { Button } from "@base-ui/react/button";
import { Field } from "@base-ui/react/field";
import { useEffect, useState } from "react";
import { type UseFormRegisterReturn, useForm } from "react-hook-form";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import { errMessage, mutateRequest } from "../lib/http";

type Feedback = { tone: "ok" | "error"; text: string } | null;

interface SettingsWire {
  timezone: string;
  announce_time: string;
  announce_enabled: boolean;
  remind_evening_time: string;
  remind_evening_enabled: boolean;
  remind_morning_time: string;
  remind_morning_enabled: boolean;
  urgent_time: string;
  urgent_enabled: boolean;
  doc_title: string;
  header_done: string;
  header_plan: string;
  header_block: string;
  column_widths: number[];
  help_template: string;
  check_template: string;
  announce_title: string;
  announce_body: string;
  reminder_title: string;
  reminder_body: string;
}

// The form mirrors the wire shape but holds column widths as an editable CSV string.
interface SettingsForm extends Omit<SettingsWire, "column_widths"> {
  column_widths: string;
}

const DEFAULT_FORM: SettingsForm = {
  timezone: "Asia/Shanghai",
  announce_time: "20:00",
  announce_enabled: true,
  remind_evening_time: "22:00",
  remind_evening_enabled: true,
  remind_morning_time: "09:30",
  remind_morning_enabled: true,
  urgent_time: "10:00",
  urgent_enabled: true,
  doc_title: "Daily Scrum - {{ date }}",
  header_done: "✅ 昨日完成",
  header_plan: "🎯 今日计划",
  header_block: "🚫 阻塞",
  column_widths: "120, 300, 300, 240",
  help_template: "",
  check_template: "",
  announce_title: "",
  announce_body: "",
  reminder_title: "",
  reminder_body: "",
};

function wireToForm(w: SettingsWire): SettingsForm {
  return { ...w, column_widths: w.column_widths.join(", ") };
}

function formToWire(f: SettingsForm): SettingsWire {
  return {
    ...f,
    timezone: f.timezone.trim(),
    column_widths: f.column_widths
      .split(",")
      .map((s) => parseInt(s.trim(), 10))
      .filter((n) => Number.isFinite(n) && n > 0),
  };
}

function ScheduleRow({
  label,
  time,
  enabled,
}: {
  label: string;
  time: UseFormRegisterReturn;
  enabled: UseFormRegisterReturn;
}) {
  return (
    <Field.Root className="field">
      <Field.Label className="field-label">{label}</Field.Label>
      <div style={{ display: "flex", gap: "0.75rem", alignItems: "center" }}>
        <input
          type="time"
          className="field-input"
          style={{ width: "auto" }}
          {...time}
        />
        <label
          style={{ display: "flex", gap: "0.35rem", alignItems: "center" }}
        >
          <input type="checkbox" {...enabled} /> enabled
        </label>
      </div>
    </Field.Root>
  );
}

function TemplateField({
  label,
  hint,
  field,
}: {
  label: string;
  hint: string;
  field: UseFormRegisterReturn;
}) {
  return (
    <Field.Root className="field">
      <Field.Label className="field-label">
        {label}
        <br />
        <span className="muted" style={{ fontWeight: 400, fontSize: "0.8em" }}>
          {hint}
        </span>
      </Field.Label>
      <textarea
        className="field-input"
        rows={4}
        style={{ fontFamily: "monospace", resize: "vertical" }}
        {...field}
      />
    </Field.Root>
  );
}

function SettingsCard() {
  const { data, error, mutate } = useSWR<SettingsWire>(
    "/api/apps/standup/settings",
  );
  const { register, handleSubmit, reset } = useForm<SettingsForm>({
    defaultValues: DEFAULT_FORM,
  });
  const [feedback, setFeedback] = useState<Feedback>(null);

  // Hydrate the form once settings load.
  useEffect(() => {
    if (data) reset(wireToForm(data));
  }, [data, reset]);

  const save = useSWRMutation(
    "/api/apps/standup/settings",
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
      {error && <p className="error">Failed to load: {errMessage(error)}</p>}
      <p className="muted help-text">
        Changes apply live — the scheduler and chat bot reload on each pass, no
        restart. Secrets &amp; bindings (chat, folder, Lark app) stay in the
        Config tab.
      </p>

      <div className="actions-subsystem">schedule</div>
      <div className="action-fields">
        <Field.Root className="field">
          <Field.Label className="field-label">Timezone (IANA)</Field.Label>
          <Field.Control
            className="field-input"
            placeholder="Asia/Shanghai"
            {...register("timezone")}
          />
        </Field.Root>
        <ScheduleRow
          label="Announce (next-day doc)"
          time={register("announce_time")}
          enabled={register("announce_enabled")}
        />
        <ScheduleRow
          label="Remind — evening (next-day)"
          time={register("remind_evening_time")}
          enabled={register("remind_evening_enabled")}
        />
        <ScheduleRow
          label="Remind — morning (today)"
          time={register("remind_morning_time")}
          enabled={register("remind_morning_enabled")}
        />
        <ScheduleRow
          label="Urgent (today)"
          time={register("urgent_time")}
          enabled={register("urgent_enabled")}
        />
      </div>

      <div className="actions-subsystem" style={{ marginTop: "1rem" }}>
        doc table
      </div>
      <div className="action-fields">
        <Field.Root className="field">
          <Field.Label className="field-label">
            Doc title
            <br />
            <span
              className="muted"
              style={{ fontWeight: 400, fontSize: "0.8em" }}
            >
              vars: {"{{ date }}"} — used to match the day's doc
            </span>
          </Field.Label>
          <Field.Control className="field-input" {...register("doc_title")} />
        </Field.Root>
        <Field.Root className="field">
          <Field.Label className="field-label">Header — done</Field.Label>
          <Field.Control className="field-input" {...register("header_done")} />
        </Field.Root>
        <Field.Root className="field">
          <Field.Label className="field-label">Header — plan</Field.Label>
          <Field.Control className="field-input" {...register("header_plan")} />
        </Field.Root>
        <Field.Root className="field">
          <Field.Label className="field-label">Header — block</Field.Label>
          <Field.Control
            className="field-input"
            {...register("header_block")}
          />
        </Field.Root>
        <Field.Root className="field">
          <Field.Label className="field-label">
            Column widths
            <br />
            <span
              className="muted"
              style={{ fontWeight: 400, fontSize: "0.8em" }}
            >
              name, done, plan, block
            </span>
          </Field.Label>
          <Field.Control
            className="field-input"
            placeholder="120, 300, 300, 240"
            {...register("column_widths")}
          />
        </Field.Root>
      </div>

      <div className="actions-subsystem" style={{ marginTop: "1rem" }}>
        templates (minijinja)
      </div>
      <div className="action-fields">
        <TemplateField
          label="Help reply"
          hint="no variables"
          field={register("help_template")}
        />
        <TemplateField
          label="Check report"
          hint="vars: date, url, missing (list)"
          field={register("check_template")}
        />
        <TemplateField
          label="Announce — title"
          hint="vars: date, days_until"
          field={register("announce_title")}
        />
        <TemplateField
          label="Announce — body"
          hint="vars: date, days_until, url"
          field={register("announce_body")}
        />
        <TemplateField
          label="Reminder — title"
          hint="vars: urgent (bool)"
          field={register("reminder_title")}
        />
        <TemplateField
          label="Reminder — body"
          hint="vars: urgent (bool), url"
          field={register("reminder_body")}
        />
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

export function Standup() {
  return (
    <section>
      <h2>Standup</h2>
      <SettingsCard />
    </section>
  );
}
