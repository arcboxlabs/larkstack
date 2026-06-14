import { Button } from "@base-ui/react/button";
import { Field } from "@base-ui/react/field";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { Link } from "react-router";
import useSWRMutation from "swr/mutation";
import { errMessage, mutateRequest } from "../lib/http";

interface ActionParam {
  name: string;
  label: string;
  required?: boolean;
  placeholder?: string;
}

interface Action {
  name: string;
  description: string;
  params?: ActionParam[];
}

const CATALOG: Record<string, Action[]> = {
  linear: [
    {
      name: "ping",
      description: "Emit a pong log event (smoke test the action plumbing)",
    },
    {
      name: "test-lark",
      description: "Post a test message to the configured Lark webhook",
    },
  ],
  github: [
    {
      name: "ping",
      description: "Emit a pong log event (smoke test the action plumbing)",
    },
    {
      name: "test-lark",
      description: "Post a test message to the configured Lark webhook",
    },
  ],
  x: [
    {
      name: "ping",
      description: "Emit a pong log event (smoke test the action plumbing)",
    },
  ],
  standup: [
    {
      name: "announce",
      description: "Ensure tomorrow's doc and post the announcement card",
      params: [
        {
          name: "date",
          label: "date (today | tomorrow | YYYY-MM-DD)",
          placeholder: "tomorrow",
        },
      ],
    },
    {
      name: "ensure",
      description: "Create tomorrow's doc + share with chat (no card)",
      params: [{ name: "date", label: "date", placeholder: "tomorrow" }],
    },
    {
      name: "remind",
      description: "DM everyone still empty for today's doc",
      params: [{ name: "date", label: "date", placeholder: "today" }],
    },
    {
      name: "urgent",
      description: "Remind + in-app urgent escalation for today's doc",
      params: [{ name: "date", label: "date", placeholder: "today" }],
    },
    {
      name: "check",
      description: "List missing fillers for today (read-only)",
      params: [{ name: "date", label: "date", placeholder: "today" }],
    },
    {
      name: "urgent-user",
      description: "Escalate one specific user (for testing)",
      params: [
        {
          name: "open_id",
          label: "open_id",
          required: true,
          placeholder: "ou_xxx",
        },
        { name: "date", label: "date", placeholder: "today" },
      ],
    },
  ],
  minutes: [
    {
      name: "process-meeting",
      description: "Backfill / re-process one meeting by ID",
      params: [
        {
          name: "meeting_id",
          label: "meeting_id",
          required: true,
          placeholder: "VC meeting ID",
        },
        {
          name: "owner",
          label: "owner (optional override)",
          placeholder: "open_id",
        },
        {
          name: "url",
          label: "url (skip VC lookup, use this URL)",
          placeholder: "https://…",
        },
      ],
    },
  ],
};

type RunState = { tone: "ok" | "error"; text: string } | null;

function ActionCard({
  subsystem,
  action,
}: {
  subsystem: string;
  action: Action;
}) {
  const params = action.params ?? [];
  const defaults: Record<string, string> = {};
  for (const p of params) defaults[p.name] = "";

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<Record<string, string>>({ defaultValues: defaults });
  const [result, setResult] = useState<RunState>(null);

  const fire = useSWRMutation(
    `action:${subsystem}/${action.name}`,
    (_key: string, { arg }: { arg: Record<string, string> | null }) =>
      mutateRequest(`/api/actions/${subsystem}/${action.name}`, { json: arg }),
    { revalidate: false, populateCache: false },
  );

  const onRun = handleSubmit(async (values) => {
    setResult(null);
    // Send required fields always; drop empty optionals so JSON carries only
    // real values (and `null` when nothing is left).
    const required = new Set(
      params.filter((p) => p.required).map((p) => p.name),
    );
    const body: Record<string, string> = {};
    for (const [k, v] of Object.entries(values)) {
      const trimmed = v.trim();
      if (required.has(k) || trimmed) body[k] = trimmed;
    }
    try {
      await fire.trigger(Object.keys(body).length ? body : null);
      setResult({ tone: "ok", text: "dispatched" });
      window.setTimeout(() => setResult(null), 2500);
    } catch (e) {
      setResult({ tone: "error", text: errMessage(e) });
    }
  });

  return (
    <div className="action-card">
      <div className="action-card-head">
        <div>
          <code className="action-name">{action.name}</code>
          <div className="muted help-text">{action.description}</div>
        </div>
        <Button
          className={`action-btn ${result?.tone ?? ""}`}
          type="button"
          onClick={onRun}
          disabled={fire.isMutating}
        >
          {fire.isMutating ? "…" : "Run"}
        </Button>
      </div>
      {params.length > 0 && (
        <div className="action-fields">
          {params.map((p) => (
            <Field.Root
              key={p.name}
              className="field"
              invalid={!!errors[p.name]}
            >
              <Field.Label className="field-label">
                {p.label}
                {p.required && <span className="req"> *</span>}
              </Field.Label>
              <Field.Control
                className="field-input"
                placeholder={p.placeholder}
                {...register(
                  p.name,
                  p.required ? { required: `${p.name} is required` } : {},
                )}
              />
              {errors[p.name] && (
                <Field.Error className="field-error" match>
                  {errors[p.name]?.message}
                </Field.Error>
              )}
            </Field.Root>
          ))}
        </div>
      )}
      {result && (
        <div className={`action-result ${result.tone}`}>{result.text}</div>
      )}
    </div>
  );
}

export function Actions() {
  return (
    <section>
      <h2>Actions</h2>
      <p className="muted help-text">
        Dispatch is fire-and-forget. The outcome of each action shows up in the{" "}
        <Link to="/events">Events</Link> tab.
      </p>
      {Object.entries(CATALOG).map(([subsystem, actions]) => (
        <div key={subsystem} className="actions-group">
          <div className="actions-subsystem">{subsystem}</div>
          {actions.length === 0 ? (
            <div className="muted help-text">no actions defined yet</div>
          ) : (
            <div className="action-cards">
              {actions.map((a) => (
                <ActionCard key={a.name} subsystem={subsystem} action={a} />
              ))}
            </div>
          )}
        </div>
      ))}
    </section>
  );
}
