import { Combobox } from "@base-ui/react/combobox";
import { Input } from "@base-ui/react/input";
import { useEffect, useRef, useState } from "react";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import { errMessage, mutateRequest } from "../lib/http";
import { Select } from "./Select";
import { Spinner } from "./Spinner";

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
interface User {
  open_id: string;
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
  /**
   * Show the reviewer user-map section (default `true`). Linear keeps its own
   * DB-backed Linear→Lark email map, so it hides this routing-blob one.
   */
  showUserMap?: boolean;
  /** Show the alert-labels section (default `true`). Unused by apps without
   * label-triggered alerts (e.g. Linear). */
  showAlertLabels?: boolean;
}

export function RoutingEditor({
  appName,
  eventOptions,
  showUserMap = true,
  showAlertLabels = true,
}: RoutingEditorProps) {
  const url = `/api/apps/${appName}/routing`;
  const { data, error, mutate } = useSWR<RoutingConfig>(url);
  // The bot's chats + reachable users power the searchable pickers; absent (503)
  // when the app is stopped or has no bot — the fields then fall back to manual
  // entry. `shouldRetryOnError: false` so a stopped app doesn't retry-storm.
  const { data: chats } = useSWR<Chat[]>(`/api/apps/${appName}/chats`, {
    shouldRetryOnError: false,
  });
  const { data: users } = useSWR<User[]>(`/api/apps/${appName}/users`, {
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
    return <Spinner />;
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

  const addRule = () =>
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
    );
  const removeRule = (key: number) =>
    setEdit((s) =>
      s ? { ...s, rules: s.rules.filter((r) => r.key !== key) } : s,
    );

  return (
    <div className="action-card routing-editor">
      <p className="routing-lead">
        Route events to Lark group chats or DMs — pick from the bot's chats and
        users, or type a <code>chat_id</code> / email. Changes apply live, no
        restart. Delivery needs a bound <code>lark_app</code> bot.
      </p>

      {/* ── Rules ── */}
      <section className="routing-section">
        <div className="routing-section-head">
          <span className="routing-section-title">Routing rules</span>
          <span className="routing-section-hint">
            every matching rule contributes its destinations
          </span>
        </div>

        {edit.rules.length === 0 && (
          <p className="routing-empty">
            No rules yet — unmatched events fall through to the defaults below.
          </p>
        )}

        {edit.rules.map((rule, i) => (
          <div key={rule.key} className="routing-rule">
            <div className="routing-rule-head">
              <span className="routing-rule-badge">Rule {i + 1}</span>
              <button
                type="button"
                className="routing-remove-rule"
                onClick={() => removeRule(rule.key)}
              >
                Remove
              </button>
            </div>

            <div className="routing-field">
              <label
                className="routing-field-label"
                htmlFor={`match-${rule.key}`}
              >
                Match
                <span className="routing-field-hint">
                  exact, “group/*”, or “*” for all
                </span>
              </label>
              <Input
                id={`match-${rule.key}`}
                className="routing-input"
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
            </div>

            <div className="routing-field">
              <span className="routing-field-label">
                Events
                <span className="routing-field-hint">none selected = all</span>
              </span>
              <div className="routing-chips">
                {eventOptions.map((opt) => {
                  const active = rule.events.includes(opt.value);
                  return (
                    <button
                      key={opt.value}
                      type="button"
                      className="routing-chip"
                      data-active={active}
                      aria-pressed={active}
                      onClick={() =>
                        setEdit((s) =>
                          patchRule(s, rule.key, (r) => ({
                            ...r,
                            events: toggle(r.events, opt.value),
                          })),
                        )
                      }
                    >
                      {active && <span className="routing-chip-check">✓</span>}
                      {opt.label}
                    </button>
                  );
                })}
              </div>
            </div>

            <DestinationList
              dests={rule.destinations}
              chats={chats}
              users={users}
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
          </div>
        ))}

        <button type="button" className="routing-add" onClick={addRule}>
          <span className="routing-add-icon">+</span> Add rule
        </button>
      </section>

      {/* ── Default destinations ── */}
      <section className="routing-section">
        <div className="routing-section-head">
          <span className="routing-section-title">Default destinations</span>
          <span className="routing-section-hint">
            used when no rule matches — empty drops the event
          </span>
        </div>
        <DestinationList
          dests={edit.default_destinations}
          chats={chats}
          users={users}
          hideLabel
          onChange={(ds) =>
            setEdit((s) => (s ? { ...s, default_destinations: ds } : s))
          }
          onAdd={() =>
            setEdit((s) =>
              s
                ? {
                    ...s,
                    default_destinations: [
                      ...s.default_destinations,
                      newDest(),
                    ],
                  }
                : s,
            )
          }
        />
      </section>

      {/* ── Reviewer user map ── */}
      {showUserMap && (
        <section className="routing-section">
          <div className="routing-section-head">
            <span className="routing-section-title">Reviewer user map</span>
            <span className="routing-section-hint">
              source username → Lark email
            </span>
          </div>
          {edit.user_map.map((m) => (
            <div key={m.key} className="routing-dest">
              <Input
                className="routing-input"
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
              <span className="routing-arrow">→</span>
              <Input
                className="routing-input"
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
              <button
                type="button"
                className="routing-icon-btn"
                aria-label="Remove mapping"
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
                ×
              </button>
            </div>
          ))}
          <button
            type="button"
            className="routing-add subtle"
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
            <span className="routing-add-icon">+</span> Add mapping
          </button>
        </section>
      )}

      {/* ── Alert labels ── */}
      {showAlertLabels && (
        <section className="routing-section">
          <div className="routing-section-head">
            <span className="routing-section-title">Alert labels</span>
            <span className="routing-section-hint">
              comma-separated; these labels trigger an alert card
            </span>
          </div>
          <Input
            className="routing-input"
            style={{ width: "100%" }}
            placeholder="bug, urgent, p0"
            value={edit.alert_labels}
            onChange={(e) =>
              setEdit((s) => (s ? { ...s, alert_labels: e.target.value } : s))
            }
          />
        </section>
      )}

      <div className="routing-footer">
        <button
          type="button"
          className="routing-save"
          onClick={onSave}
          disabled={save.isMutating}
        >
          {save.isMutating ? "Saving…" : "Save routing"}
        </button>
        {feedback && (
          <span className={`routing-feedback ${feedback.tone}`}>
            {feedback.text}
          </span>
        )}
      </div>
    </div>
  );
}

function DestinationList({
  dests,
  chats,
  users,
  onChange,
  onAdd,
  hideLabel = false,
}: {
  dests: DestRow[];
  chats: Chat[] | undefined;
  users: User[] | undefined;
  onChange: (ds: DestRow[]) => void;
  onAdd: () => void;
  /** Omit the "Destinations" sub-label (the section header already names it). */
  hideLabel?: boolean;
}) {
  const patch = (key: number, fn: (d: DestRow) => DestRow) =>
    onChange(dests.map((x) => (x.key === key ? fn(x) : x)));
  // Picker sources: chats keyed by chat_id, users keyed by open_id.
  const chatItems = chats?.map((c) => ({ value: c.chat_id, label: c.name }));
  const userItems = users?.map((u) => ({ value: u.open_id, label: u.name }));
  return (
    <div className="routing-dests">
      {!hideLabel && <span className="routing-dest-label">Destinations</span>}
      {dests.map((d) => (
        <div key={d.key} className="routing-dest">
          <Select
            className="routing-kind select-trigger"
            value={d.kind}
            onValueChange={(v) =>
              // Switching kind clears the target — a chat_id and a user id aren't
              // interchangeable, and the picker source differs.
              patch(d.key, (x) => ({ ...x, kind: v as DestKind, target: "" }))
            }
            options={[
              { value: "chat", label: "Group chat" },
              { value: "dm", label: "Direct message" },
            ]}
          />
          <PickerField
            items={d.kind === "chat" ? chatItems : userItems}
            value={d.target}
            onChange={(target) => patch(d.key, (x) => ({ ...x, target }))}
            searchPlaceholder={
              d.kind === "chat" ? "Search group chats…" : "Search users…"
            }
            manualPlaceholder={
              d.kind === "chat" ? "chat_id (oc_…)" : "open_id / email"
            }
            emptyLabel={
              d.kind === "chat" ? "No matching chats" : "No matching users"
            }
          />
          <button
            type="button"
            className="routing-icon-btn"
            aria-label="Remove destination"
            onClick={() => onChange(dests.filter((x) => x.key !== d.key))}
          >
            ×
          </button>
        </div>
      ))}
      <button type="button" className="routing-add subtle" onClick={onAdd}>
        <span className="routing-add-icon">+</span> Add destination
      </button>
    </div>
  );
}

interface PickerItem {
  /** The stored value: a chat_id or a user open_id. */
  value: string;
  /** The human label: a chat or user display name. */
  label: string;
}

/**
 * A destination-target field: a searchable Select over `items` (the bot's chats
 * or reachable users), matched by display name but storing the underlying id.
 * Since the bot can only deliver to chats/users it can reach, picking from the
 * fetched list is also the correct constraint. Falls back to a plain text input
 * when the list is unavailable (app stopped / no bot / 503), so a `chat_id`,
 * `open_id`, or email can still be entered by hand.
 */
function PickerField({
  items,
  value,
  onChange,
  searchPlaceholder,
  manualPlaceholder,
  emptyLabel,
}: {
  items: PickerItem[] | undefined;
  value: string;
  onChange: (value: string) => void;
  searchPlaceholder: string;
  manualPlaceholder: string;
  emptyLabel: string;
}) {
  if (!items || items.length === 0) {
    return (
      <Input
        className="routing-input"
        placeholder={manualPlaceholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }

  // Build the candidate id list keyed to labels. Include the current value as a
  // synthetic entry when it's a saved id (or email) not among the fetched items,
  // so it still shows and stays selected.
  const byId = new Map(items.map((i) => [i.value, i.label]));
  const ids = items.map((i) => i.value);
  if (value && !byId.has(value)) {
    ids.unshift(value);
    byId.set(value, value);
  }
  const labelOf = (id: string) => byId.get(id) ?? id;

  return (
    <Combobox.Root
      items={ids}
      value={value || null}
      onValueChange={(v) => onChange((v as string | null) ?? "")}
      itemToStringLabel={labelOf}
    >
      <span className="combobox-control">
        <Combobox.Input
          className="routing-input combobox-input"
          placeholder={searchPlaceholder}
        />
        <Combobox.Trigger className="combobox-trigger" aria-label="Open">
          <Combobox.Icon className="select-icon">▾</Combobox.Icon>
        </Combobox.Trigger>
      </span>
      <Combobox.Portal>
        <Combobox.Positioner sideOffset={4} align="start">
          <Combobox.Popup className="select-popup combobox-popup">
            <Combobox.Empty className="combobox-empty">
              {emptyLabel}
            </Combobox.Empty>
            <Combobox.List>
              {(id: string) => (
                <Combobox.Item
                  key={id}
                  value={id}
                  className="select-item combobox-item"
                >
                  <Combobox.ItemIndicator className="select-item-indicator">
                    ✓
                  </Combobox.ItemIndicator>
                  <span className="combobox-item-text">
                    <span>{byId.get(id) ?? id}</span>
                    <span className="combobox-item-id muted">{id}</span>
                  </span>
                </Combobox.Item>
              )}
            </Combobox.List>
          </Combobox.Popup>
        </Combobox.Positioner>
      </Combobox.Portal>
    </Combobox.Root>
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
