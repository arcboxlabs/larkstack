import { Button } from "@base-ui/react/button";
import { useEffect, useRef, useState } from "react";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import { errMessage, mutateRequest } from "../lib/http";

// ── Wire shape (matches lark_kit::routing::Config) ──────────────────────────

type DestKind = "chat" | "dm";
interface Destination {
  kind: DestKind;
  target: string;
}
interface Rule {
  match: string;
  events: string[];
  destinations: Destination[];
}
interface UserMap {
  username: string;
  lark_email: string;
}
interface RoutingConfig {
  rules: Rule[];
  default_destinations: Destination[];
  user_map: UserMap[];
  alert_labels: string[];
}
interface Chat {
  chat_id: string;
  name: string;
}

// ── Editable shape: rows carry a stable client key (avoids index keys) and
//    alert_labels is edited as a CSV string. ──────────────────────────────────

interface DestRow extends Destination {
  key: number;
}
interface RuleRow {
  key: number;
  match: string;
  events: string[];
  destinations: DestRow[];
}
interface UserMapRow extends UserMap {
  key: number;
}
interface EditState {
  rules: RuleRow[];
  default_destinations: DestRow[];
  user_map: UserMapRow[];
  alert_labels: string;
}

type Feedback = { tone: "ok" | "error"; text: string } | null;

export interface EventOption {
  value: string;
  label: string;
}

export interface RoutingEditorProps {
  /** App name; backs the API base `/api/apps/<appName>/routing`. */
  appName: string;
  /** The app's event vocabulary, shown as per-rule filter checkboxes. */
  eventOptions: EventOption[];
}

export function RoutingEditor({ appName, eventOptions }: RoutingEditorProps) {
  const url = `/api/apps/${appName}/routing`;
  const { data, error, mutate } = useSWR<RoutingConfig>(url);
  // The bot's chats power the chat-picker; absent (503) when the app is stopped
  // or has no bot — the chat fields then fall back to manual chat_id entry.
  const chatsListId = `chats-${appName}`;
  const { data: chats } = useSWR<Chat[]>(`/api/apps/${appName}/chats`, {
    shouldRetryOnError: false,
  });
  const [edit, setEdit] = useState<EditState | null>(null);
  const [feedback, setFeedback] = useState<Feedback>(null);
  const keyer = useRef(0);
  const nextKey = () => {
    keyer.current += 1;
    return keyer.current;
  };

  // Hydrate the editable state once the config loads. Uses a local key counter
  // (seeded from the ref) so the effect depends only on `data`.
  useEffect(() => {
    if (!data) return;
    let k = keyer.current;
    const nk = () => {
      k += 1;
      return k;
    };
    const dests = (ds: Destination[]): DestRow[] =>
      ds.map((d) => ({ key: nk(), kind: d.kind, target: d.target }));
    setEdit({
      rules: data.rules.map((r) => ({
        key: nk(),
        match: r.match,
        events: r.events,
        destinations: dests(r.destinations),
      })),
      default_destinations: dests(data.default_destinations),
      user_map: data.user_map.map((m) => ({
        key: nk(),
        username: m.username,
        lark_email: m.lark_email,
      })),
      alert_labels: data.alert_labels.join(", "),
    });
    keyer.current = k;
  }, [data]);

  const save = useSWRMutation(
    url,
    (u: string, { arg }: { arg: RoutingConfig }) =>
      mutateRequest<RoutingConfig>(u, { method: "PUT", json: arg }),
    {
      onSuccess: (saved) => {
        if (saved) void mutate(saved, { revalidate: false });
      },
    },
  );

  if (error) {
    return <p className="error">Failed to load: {errMessage(error)}</p>;
  }
  if (!edit) {
    return <p className="muted">Loading…</p>;
  }

  const onSave = async () => {
    setFeedback(null);
    try {
      await save.trigger(toWire(edit));
      setFeedback({ tone: "ok", text: "routing saved" });
    } catch (e) {
      setFeedback({ tone: "error", text: errMessage(e) });
    }
  };

  const newDest = (): DestRow => ({ key: nextKey(), kind: "chat", target: "" });

  return (
    <div className="action-card">
      <p className="muted help-text">
        Route events to Lark group chats (by <code>chat_id</code>) or DMs (by
        email). Changes apply live — no restart. Delivery needs a bound{" "}
        <code>lark_app</code> bot.
      </p>
      {chats && chats.length > 0 && (
        <datalist id={chatsListId}>
          {chats.map((c) => (
            <option key={c.chat_id} value={c.chat_id}>
              {c.name}
            </option>
          ))}
        </datalist>
      )}

      {/* ── Rules ── */}
      <div className="actions-subsystem">routing rules</div>
      {edit.rules.length === 0 && (
        <p className="muted help-text">
          No rules yet — unmatched events use the defaults below.
        </p>
      )}
      {edit.rules.map((rule) => (
        <div
          key={rule.key}
          className="action-card"
          style={{ marginBottom: "0.75rem" }}
        >
          <label className="field">
            <span className="field-label">
              match <span className="muted">(exact, "group/*", or "*")</span>
            </span>
            <input
              className="field-input"
              placeholder="group/*"
              value={rule.match}
              onChange={(e) =>
                setEdit((s) =>
                  patchRule(s, rule.key, (r) => ({
                    ...r,
                    match: e.target.value,
                  })),
                )
              }
            />
          </label>

          <span className="field-label">events (none = all)</span>
          <div className="filters" style={{ flexWrap: "wrap", gap: "0.75rem" }}>
            {eventOptions.map((opt) => (
              <label
                key={opt.value}
                style={{
                  display: "flex",
                  gap: "0.35rem",
                  alignItems: "center",
                }}
              >
                <input
                  type="checkbox"
                  checked={rule.events.includes(opt.value)}
                  onChange={() =>
                    setEdit((s) =>
                      patchRule(s, rule.key, (r) => ({
                        ...r,
                        events: toggle(r.events, opt.value),
                      })),
                    )
                  }
                />
                {opt.label}
              </label>
            ))}
          </div>

          <DestinationList
            dests={rule.destinations}
            chatsListId={chatsListId}
            onChange={(ds) =>
              setEdit((s) =>
                patchRule(s, rule.key, (r) => ({ ...r, destinations: ds })),
              )
            }
            onAdd={() =>
              setEdit((s) =>
                patchRule(s, rule.key, (r) => ({
                  ...r,
                  destinations: [...r.destinations, newDest()],
                })),
              )
            }
          />

          <div className="filters" style={{ marginTop: "0.5rem" }}>
            <Button
              type="button"
              className="action-btn error"
              onClick={() =>
                setEdit((s) =>
                  s
                    ? { ...s, rules: s.rules.filter((r) => r.key !== rule.key) }
                    : s,
                )
              }
            >
              Remove rule
            </Button>
          </div>
        </div>
      ))}
      <Button
        type="button"
        onClick={() =>
          setEdit((s) =>
            s
              ? {
                  ...s,
                  rules: [
                    ...s.rules,
                    {
                      key: nextKey(),
                      match: "",
                      events: [],
                      destinations: [newDest()],
                    },
                  ],
                }
              : s,
          )
        }
      >
        Add rule
      </Button>

      {/* ── Default destinations ── */}
      <div className="actions-subsystem" style={{ marginTop: "1.5rem" }}>
        default destinations (unmatched events — empty = drop)
      </div>
      <DestinationList
        dests={edit.default_destinations}
        chatsListId={chatsListId}
        onChange={(ds) =>
          setEdit((s) => (s ? { ...s, default_destinations: ds } : s))
        }
        onAdd={() =>
          setEdit((s) =>
            s
              ? {
                  ...s,
                  default_destinations: [...s.default_destinations, newDest()],
                }
              : s,
          )
        }
      />

      {/* ── Reviewer user map ── */}
      <div className="actions-subsystem" style={{ marginTop: "1.5rem" }}>
        reviewer user map (source username → Lark email)
      </div>
      {edit.user_map.map((m) => (
        <div key={m.key} className="filters" style={{ marginBottom: "0.4rem" }}>
          <input
            className="field-input"
            placeholder="username"
            value={m.username}
            onChange={(e) =>
              setEdit((s) =>
                patchUser(s, m.key, (u) => ({
                  ...u,
                  username: e.target.value,
                })),
              )
            }
          />
          <input
            className="field-input"
            placeholder="lark@email"
            value={m.lark_email}
            onChange={(e) =>
              setEdit((s) =>
                patchUser(s, m.key, (u) => ({
                  ...u,
                  lark_email: e.target.value,
                })),
              )
            }
          />
          <Button
            type="button"
            className="action-btn error"
            onClick={() =>
              setEdit((s) =>
                s
                  ? {
                      ...s,
                      user_map: s.user_map.filter((u) => u.key !== m.key),
                    }
                  : s,
              )
            }
          >
            Remove
          </Button>
        </div>
      ))}
      <Button
        type="button"
        onClick={() =>
          setEdit((s) =>
            s
              ? {
                  ...s,
                  user_map: [
                    ...s.user_map,
                    { key: nextKey(), username: "", lark_email: "" },
                  ],
                }
              : s,
          )
        }
      >
        Add mapping
      </Button>

      {/* ── Alert labels ── */}
      <label className="field" style={{ marginTop: "1.5rem" }}>
        <span className="field-label">alert labels (comma-separated)</span>
        <input
          className="field-input"
          placeholder="bug, urgent, p0"
          value={edit.alert_labels}
          onChange={(e) =>
            setEdit((s) => (s ? { ...s, alert_labels: e.target.value } : s))
          }
        />
      </label>

      <div className="filters" style={{ marginTop: "1rem" }}>
        <Button type="button" onClick={onSave} disabled={save.isMutating}>
          {save.isMutating ? "Saving…" : "Save routing"}
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

function DestinationList({
  dests,
  chatsListId,
  onChange,
  onAdd,
}: {
  dests: DestRow[];
  chatsListId: string;
  onChange: (ds: DestRow[]) => void;
  onAdd: () => void;
}) {
  return (
    <div style={{ marginTop: "0.5rem" }}>
      <span className="field-label">destinations</span>
      {dests.map((d) => (
        <div key={d.key} className="filters" style={{ marginBottom: "0.4rem" }}>
          <select
            className="field-input"
            style={{ width: "auto" }}
            value={d.kind}
            onChange={(e) =>
              onChange(
                dests.map((x) =>
                  x.key === d.key
                    ? { ...x, kind: e.target.value as DestKind }
                    : x,
                ),
              )
            }
          >
            <option value="chat">Group chat</option>
            <option value="dm">Direct message</option>
          </select>
          <input
            className="field-input"
            list={d.kind === "chat" ? chatsListId : undefined}
            placeholder={d.kind === "chat" ? "chat_id (oc_…)" : "user@email"}
            value={d.target}
            onChange={(e) =>
              onChange(
                dests.map((x) =>
                  x.key === d.key ? { ...x, target: e.target.value } : x,
                ),
              )
            }
          />
          <Button
            type="button"
            className="action-btn error"
            onClick={() => onChange(dests.filter((x) => x.key !== d.key))}
          >
            Remove
          </Button>
        </div>
      ))}
      <Button type="button" className="action-btn" onClick={onAdd}>
        Add destination
      </Button>
    </div>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function toggle(list: string[], value: string): string[] {
  return list.includes(value)
    ? list.filter((v) => v !== value)
    : [...list, value];
}

function patchRule(
  s: EditState | null,
  key: number,
  fn: (r: RuleRow) => RuleRow,
): EditState | null {
  if (!s) return s;
  return { ...s, rules: s.rules.map((r) => (r.key === key ? fn(r) : r)) };
}

function patchUser(
  s: EditState | null,
  key: number,
  fn: (u: UserMapRow) => UserMapRow,
): EditState | null {
  if (!s) return s;
  return { ...s, user_map: s.user_map.map((u) => (u.key === key ? fn(u) : u)) };
}

function toWire(e: EditState): RoutingConfig {
  const dest = (d: DestRow): Destination => ({
    kind: d.kind,
    target: d.target.trim(),
  });
  return {
    rules: e.rules.map((r) => ({
      match: r.match.trim(),
      events: r.events,
      destinations: r.destinations.map(dest).filter((d) => d.target.length > 0),
    })),
    default_destinations: e.default_destinations
      .map(dest)
      .filter((d) => d.target.length > 0),
    user_map: e.user_map
      .map((m) => ({
        username: m.username.trim(),
        lark_email: m.lark_email.trim(),
      }))
      .filter((m) => m.username.length > 0 && m.lark_email.length > 0),
    alert_labels: e.alert_labels
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0),
  };
}
