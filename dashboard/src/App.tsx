import { useEffect, useState } from "react";
import { Actions } from "./Actions";
import { Config } from "./Config";
import { Events } from "./Events";
import { LarkApps } from "./LarkApps";
import { Login } from "./Login";
import { Status } from "./Status";
import { getToken, setToken, subscribe } from "./auth";

type Tab = "status" | "actions" | "lark-apps" | "config" | "events";

const TABS: Array<{ id: Tab; label: string }> = [
  { id: "status", label: "Status" },
  { id: "actions", label: "Actions" },
  { id: "lark-apps", label: "Lark Apps" },
  { id: "config", label: "Config" },
  { id: "events", label: "Events" },
];

function parseHash(): Tab {
  const h = window.location.hash.slice(1);
  return (TABS.find((t) => t.id === h)?.id ?? "status") as Tab;
}

export function App() {
  const [token, setTok] = useState<string | null>(getToken());
  const [authMode, setAuthMode] = useState<"unknown" | "open" | "secured">(
    "unknown",
  );
  const [tab, setTab] = useState<Tab>(parseHash());

  useEffect(() => subscribe(() => setTok(getToken())), []);

  useEffect(() => {
    const onHash = () => setTab(parseHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  useEffect(() => {
    fetch("/api/status")
      .then((r) => {
        if (r.status === 401) setAuthMode("secured");
        else if (r.ok) setAuthMode("open");
        else setAuthMode("secured");
      })
      .catch(() => setAuthMode("secured"));
  }, []);

  const navigate = (id: Tab) => {
    window.location.hash = id;
    setTab(id);
  };

  if (authMode === "unknown") {
    return (
      <main>
        <p>Loading…</p>
      </main>
    );
  }
  if (authMode === "secured" && !token) {
    return <Login onAuthed={() => setTok(getToken())} />;
  }

  return (
    <div className="app">
      <header className="app-header">
        <div className="app-title">larkstack console</div>
        <nav className="tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={`tab ${tab === t.id ? "active" : ""}`}
              onClick={() => navigate(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
        {token && (
          <button
            className="signout"
            onClick={() => setToken(null)}
            title="Sign out"
          >
            sign out
          </button>
        )}
      </header>
      <main>
        {tab === "status" && <Status />}
        {tab === "actions" && <Actions />}
        {tab === "lark-apps" && <LarkApps />}
        {tab === "config" && <Config />}
        {tab === "events" && <Events />}
      </main>
    </div>
  );
}
