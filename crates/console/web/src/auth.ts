const KEY = "larkstack.token";

let cached: string | null = null;
const listeners = new Set<() => void>();

export function getToken(): string | null {
  if (cached !== null) return cached;
  try {
    cached = localStorage.getItem(KEY);
  } catch {
    cached = null;
  }
  return cached;
}

export function setToken(t: string | null) {
  cached = t;
  try {
    if (t === null) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, t);
  } catch {
    // private mode: just keep in-memory
  }
  listeners.forEach((fn) => fn());
}

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/// `fetch` wrapper that injects the bearer token and surfaces 401 by
/// clearing the cached token so the UI falls back to the login screen.
export async function api(
  input: string,
  init: RequestInit = {},
): Promise<Response> {
  const t = getToken();
  const headers = new Headers(init.headers);
  if (t) headers.set("Authorization", `Bearer ${t}`);
  const r = await fetch(input, { ...init, headers });
  if (r.status === 401) {
    setToken(null);
  }
  return r;
}

/// EventSource doesn't support custom headers, so the token rides as a query
/// param. URL-encode just in case the token contains reserved characters.
export function sseUrl(path: string): string {
  const t = getToken();
  if (!t) return path;
  const sep = path.includes("?") ? "&" : "?";
  return `${path}${sep}token=${encodeURIComponent(t)}`;
}
