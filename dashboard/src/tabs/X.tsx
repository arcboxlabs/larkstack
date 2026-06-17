import { Link } from "react-router";
import { LarkBinding } from "../components/LarkBinding";

/// X (Twitter) is preview-only: it has no notification routing, so its only
/// console-editable setting is which Lark app replies to the link previews.
export function X() {
  return (
    <section>
      <h2>X</h2>
      <p className="muted help-text">
        Link-preview integration. When an X/Twitter URL is shared in Lark, this
        app fetches the tweet and replies with a preview card — so a Lark app
        must be bound for it to answer. There is no notification routing; the
        only console setting is the binding below. Secrets (e.g.{" "}
        <code>X_BEARER_TOKEN</code>) come from the environment or the{" "}
        <Link to="/config">Config</Link> tab.
      </p>
      <LarkBinding appName="x" />
    </section>
  );
}
