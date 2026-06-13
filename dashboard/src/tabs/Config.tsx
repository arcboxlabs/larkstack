import { useEffect, useState } from "react";
import { api } from "../lib/auth";

type SaveState =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "saved" }
  | { kind: "error"; message: string };

export function Config() {
  const [original, setOriginal] = useState<string | null>(null);
  const [draft, setDraft] = useState<string>("");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [save, setSave] = useState<SaveState>({ kind: "idle" });

  useEffect(() => {
    api("/api/config")
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.text();
      })
      .then((text) => {
        setOriginal(text);
        setDraft(text);
        setLoadError(null);
      })
      .catch((e) => setLoadError(String(e)));
  }, []);

  const dirty = original !== null && draft !== original;

  const onSave = async () => {
    setSave({ kind: "saving" });
    try {
      const r = await api("/api/config", {
        method: "PUT",
        headers: { "Content-Type": "application/toml" },
        body: draft,
      });
      if (!r.ok) {
        const j = (await r.json().catch(() => null)) as { error?: string } | null;
        throw new Error(j?.error ?? `HTTP ${r.status}`);
      }
      setOriginal(draft);
      setSave({ kind: "saved" });
      window.setTimeout(() => setSave({ kind: "idle" }), 2000);
    } catch (e) {
      setSave({ kind: "error", message: String(e) });
    }
  };

  return (
    <section>
      <header className="events-header">
        <h2>Configuration</h2>
        <div className="filters">
          {save.kind === "error" && (
            <span className="error">{save.message}</span>
          )}
          {save.kind === "saved" && <span className="conn ok">saved</span>}
          <button
            onClick={onSave}
            disabled={!dirty || save.kind === "saving" || original === null}
          >
            {save.kind === "saving" ? "Saving…" : "Save"}
          </button>
        </div>
      </header>
      {loadError && <p className="error">Failed to load: {loadError}</p>}
      <textarea
        className="config-editor"
        spellCheck={false}
        value={draft}
        disabled={original === null}
        onChange={(e) => setDraft(e.target.value)}
      />
    </section>
  );
}
