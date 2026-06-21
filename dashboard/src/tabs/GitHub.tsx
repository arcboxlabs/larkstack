import { LarkBinding } from "../components/LarkBinding";
import { RoutingEditor } from "../components/RoutingEditor";

export function GitHub() {
  return (
    <section>
      <h2>GitHub</h2>
      <LarkBinding appName="github" />
      <RoutingEditor appName="github" />
    </section>
  );
}
