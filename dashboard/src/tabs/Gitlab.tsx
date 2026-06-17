import { LarkBinding } from "../components/LarkBinding";
import { type EventOption, RoutingEditor } from "../components/RoutingEditor";

const GITLAB_EVENTS: EventOption[] = [
  { value: "merge_request", label: "Merge requests" },
  { value: "issue", label: "Issues (alert labels)" },
  { value: "pipeline", label: "Pipeline failures" },
  { value: "note", label: "Comments" },
  { value: "push", label: "Pushes" },
];

export function Gitlab() {
  return (
    <section>
      <h2>GitLab</h2>
      <LarkBinding appName="gitlab" />
      <RoutingEditor appName="gitlab" eventOptions={GITLAB_EVENTS} />
    </section>
  );
}
