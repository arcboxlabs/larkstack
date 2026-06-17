import { Link } from "react-router";
import { LarkBinding } from "../components/LarkBinding";

/// Minutes is an automation driven by Lark VC events and the `process-meeting`
/// action; its config (STT backend, folder, secrets) lives in env / the Config
/// tab. The one console-editable binding is the Lark app it delivers with.
export function Minutes() {
  return (
    <section>
      <h2>Minutes</h2>
      <p className="muted help-text">
        Auto-transcribes recorded Lark/Feishu meetings and posts a digest card.
        It runs on Lark VC events; you can also trigger it on demand with the{" "}
        <Link to="/actions">process-meeting</Link> action. STT backend, output
        folder, and secrets come from the environment or the{" "}
        <Link to="/config">Config</Link> tab — the binding below is which Lark
        app it uses to fetch recordings and post digests.
      </p>
      <LarkBinding appName="minutes" />
    </section>
  );
}
