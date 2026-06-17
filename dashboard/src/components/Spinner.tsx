/// A pure-CSS loading spinner. Pass `fullscreen` to center it in the viewport
/// for route-/app-level loading states; otherwise it renders inline.
export function Spinner({
  fullscreen = false,
  label = "Loading",
}: {
  fullscreen?: boolean;
  label?: string;
}) {
  const spinner = <span className="spinner" role="status" aria-label={label} />;
  if (!fullscreen) return spinner;
  return <div className="spinner-screen">{spinner}</div>;
}
