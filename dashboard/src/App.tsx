import { Navigate, Route, Routes } from "react-router";
import { Layout } from "./components/Layout";
import { Login } from "./Login";
import { useMe } from "./lib/auth";
import { Actions } from "./tabs/Actions";
import { Config } from "./tabs/Config";
import { Events } from "./tabs/Events";
import { LarkApps } from "./tabs/LarkApps";
import { Linear } from "./tabs/Linear";
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

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Navigate to="/status" replace />} />
        <Route path="status" element={<Status />} />
        <Route path="actions" element={<Actions />} />
        <Route path="lark-apps" element={<LarkApps />} />
        <Route path="linear" element={<Linear />} />
        <Route path="config" element={<Config />} />
        <Route path="events" element={<Events />} />
        <Route path="*" element={<Navigate to="/status" replace />} />
      </Route>
    </Routes>
  );
}
