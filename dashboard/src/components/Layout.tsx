import { Button } from "@base-ui/react/button";
import { Tabs } from "@base-ui/react/tabs";
import { Link, Outlet, useLocation } from "react-router";
import { logout, useMe } from "../lib/auth";

const TABS = [
  { to: "/status", label: "Status" },
  { to: "/actions", label: "Actions" },
  { to: "/lark-apps", label: "Lark Apps" },
  { to: "/linear", label: "Linear" },
  { to: "/config", label: "Config" },
  { to: "/events", label: "Events" },
] as const;

/// The console shell: a Base UI Tabs bar whose value is driven by the URL (each
/// tab is rendered AS a react-router `<Link>`), plus the routed content via
/// `<Outlet>`. The router stays the single source of truth — Base UI supplies
/// the widget semantics, keyboard navigation, and active styling.
export function Layout() {
  const { pathname } = useLocation();
  const current = TABS.find((t) => pathname.startsWith(t.to))?.to ?? "/status";
  const { me } = useMe();
  const who = me?.user?.name || me?.user?.email;

  return (
    <div className="app">
      <header className="app-header">
        <div className="app-title">LarkStack Console</div>
        <Tabs.Root value={current}>
          <Tabs.List className="tabs">
            {TABS.map((t) => (
              <Tabs.Tab
                key={t.to}
                value={t.to}
                nativeButton={false}
                className={(state) => (state.active ? "tab active" : "tab")}
                render={<Link to={t.to} />}
              >
                {t.label}
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.Root>
        {me?.authenticated && (
          <div className="session">
            {who && (
              <span className="user-chip" title={me.user?.email}>
                {who}
              </span>
            )}
            <Button
              className="signout"
              type="button"
              onClick={() => logout()}
              title="Sign out"
            >
              sign out
            </Button>
          </div>
        )}
      </header>
      <main>
        <Outlet />
      </main>
    </div>
  );
}
