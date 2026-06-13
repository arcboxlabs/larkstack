import { useEffect, useState } from "react";
import { Tabs } from "@base-ui/react/tabs";
import { TabBar, type TabDef } from "./components/TabBar";
import { Actions } from "./tabs/Actions";
import { Config } from "./tabs/Config";
import { Events } from "./tabs/Events";
import { LarkApps } from "./tabs/LarkApps";
import { Status } from "./tabs/Status";
import { Login } from "./Login";
import { getToken, setToken, subscribe } from "./lib/auth";

type Tab = "status" | "actions" | "lark-apps" | "config" | "events";

const TABS: ReadonlyArray<TabDef & { id: Tab }> = [
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
    if (id === tab) return;
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
      <Tabs.Root value={tab} onValueChange={(value) => navigate(value as Tab)}>
        <header className="app-header">
          <div className="app-title">larkstack console</div>
          <TabBar tabs={TABS} />
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
          <Tabs.Panel value="status">
            <Status />
          </Tabs.Panel>
          <Tabs.Panel value="actions">
            <Actions />
          </Tabs.Panel>
          <Tabs.Panel value="lark-apps">
            <LarkApps />
          </Tabs.Panel>
          <Tabs.Panel value="config">
            <Config />
          </Tabs.Panel>
          <Tabs.Panel value="events">
            <Events />
          </Tabs.Panel>
        </main>
      </Tabs.Root>
    </div>
  );
}
