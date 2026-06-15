import { Navigate, Route, Routes } from "react-router";
import { Layout } from "./components/Layout";
import { Login } from "./Login";
import { useMe } from "./lib/auth";
import { Actions } from "./tabs/Actions";
import { Config } from "./tabs/Config";
import { Events } from "./tabs/Events";
import { GitHub } from "./tabs/GitHub";
import { Gitlab } from "./tabs/Gitlab";
import { LarkApps } from "./tabs/LarkApps";
import { Linear } from "./tabs/Linear";
import { Setup } from "./tabs/Setup";
import { Standup } from "./tabs/Standup";
import { Status } from "./tabs/Status";

export function App() {
  const { me } = useMe();

  if (!me) {
    return (
      <main>
        <p>Loading…</p>
      </main>
    );
  }
  if (me.auth_required && !me.authenticated) {
    return <Login />;
  }

  // First run (console still open) lands on the guided Setup screen; once
  // sign-in is enforced, the default view is Status.
  const home = me.auth_required ? "/status" : "/setup";

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Navigate to={home} replace />} />
        <Route path="setup" element={<Setup />} />
        <Route path="status" element={<Status />} />
        <Route path="actions" element={<Actions />} />
        <Route path="lark-apps" element={<LarkApps />} />
        <Route path="linear" element={<Linear />} />
        <Route path="github" element={<GitHub />} />
        <Route path="gitlab" element={<Gitlab />} />
        <Route path="standup" element={<Standup />} />
        <Route path="config" element={<Config />} />
        <Route path="events" element={<Events />} />
        <Route path="*" element={<Navigate to={home} replace />} />
      </Route>
    </Routes>
  );
}
