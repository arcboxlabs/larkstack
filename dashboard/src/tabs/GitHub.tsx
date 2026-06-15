import { type EventOption, RoutingEditor } from "../components/RoutingEditor";

const GITHUB_EVENTS: EventOption[] = [
  { value: "pull_request", label: "Pull requests" },
  { value: "issues", label: "Issues (alert labels)" },
  { value: "workflow_run", label: "CI failures" },
  { value: "secret_scanning", label: "Secret scanning" },
  { value: "dependabot", label: "Dependabot alerts" },
];

export function GitHub() {
  return (
    <section>
      <h2>GitHub</h2>
      <RoutingEditor appName="github" eventOptions={GITHUB_EVENTS} />
    </section>
  );
}
