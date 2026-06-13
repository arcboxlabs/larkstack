import { useEffect, useState } from "react";
import { Tabs } from "@base-ui/react/tabs";
import { TabBar, type TabDef } from "./components/TabBar";
import { Actions } from "./tabs/Actions";
import { Config } from "./tabs/Config";
import { Events } from "./tabs/Events";
import { LarkApps } from "./tabs/LarkApps";
import { Status } from "./tabs/Status";
import { Login } from "./Login";
import { getMe, logout, refreshMe, subscribe, type Me } from "./lib/auth";

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
  const [me, setMe] = useState<Me | null>(getMe());
  const [tab, setTab] = useState<Tab>(parseHash());

  useEffect(() => subscribe(() => setMe(getMe())), []);
  useEffect(() => {
    refreshMe();
  }, []);

  useEffect(() => {
    const onHash = () => setTab(parseHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const navigate = (id: Tab) => {
    if (id === tab) return;
    window.location.hash = id;
    setTab(id);
  };

  if (me === null) {
    return (
      <main>
        <p>Loading…</p>
      </main>
    );
  }
  if (me.auth_required && !me.authenticated) {
    return <Login />;
  }

  const who = me.user?.name || me.user?.email;

  return (
    <div className="app">
      <Tabs.Root value={tab} onValueChange={(value) => navigate(value as Tab)}>
        <header className="app-header">
          <div className="app-title">larkstack console</div>
          <TabBar tabs={TABS} />
          {me.authenticated && (
            <div className="session">
              {who && (
                <span className="user-chip" title={me.user?.email}>
                  {who}
                </span>
              )}
              <button
                className="signout"
                type="button"
                onClick={() => logout()}
                title="Sign out"
              >
                sign out
              </button>
            </div>
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
