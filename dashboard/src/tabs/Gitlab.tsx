import { LarkBinding } from "../components/LarkBinding";
import { RoutingEditor } from "../components/RoutingEditor";

export function Gitlab() {
  return (
    <section>
      <h2>GitLab</h2>
      <LarkBinding appName="gitlab" />
      <RoutingEditor appName="gitlab" />
    </section>
  );
}
