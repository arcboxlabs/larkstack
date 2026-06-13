/// Console auth state. The session lives in an HttpOnly cookie set by the Lark
/// OAuth flow; the browser sends it automatically, so there is no token to
/// store. `/auth/me` reports whether OAuth is configured and who is signed in.

export type Me = {
  /// False when OAuth is unconfigured — the console is open, no login needed.
  auth_required: boolean;
  authenticated: boolean;
  user?: { email: string; name: string };
};

let current: Me | null = null;
const listeners = new Set<() => void>();

export function getMe(): Me | null {
  return current;
}

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export async function refreshMe(): Promise<Me> {
  try {
    const r = await fetch("/auth/me", { credentials: "same-origin" });
    current = (await r.json()) as Me;
  } catch {
    current = { auth_required: true, authenticated: false };
  }
  listeners.forEach((fn) => fn());
  return current;
}

/// `fetch` wrapper for `/api/*`. The session rides in the cookie, so no header
/// is set; a 401 means it lapsed — re-probe so the UI falls back to login.
export async function api(
  input: string,
  init: RequestInit = {},
): Promise<Response> {
  const r = await fetch(input, { ...init, credentials: "same-origin" });
  if (r.status === 401) refreshMe();
  return r;
}

/// EventSource sends the session cookie for same-origin requests, so the SSE
/// URL needs no token.
export function sseUrl(path: string): string {
  return path;
}

/// Redirect the browser into the Lark OAuth flow.
export function login(): void {
  window.location.assign("/auth/login");
}

export async function logout(): Promise<void> {
  await fetch("/auth/logout", { method: "POST", credentials: "same-origin" });
  await refreshMe();
}
