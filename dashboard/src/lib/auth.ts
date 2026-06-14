/// Console auth state. The session lives in an HttpOnly cookie set by the Lark
/// OAuth flow; the browser sends it automatically, so there is no token to
/// store. `/auth/me` reports whether OAuth is configured and who is signed in,
/// and is read through SWR so the App gate and the header chip share one probe.

import useSWR, { mutate } from "swr";

export type Me = {
  /// False when OAuth is unconfigured — the console is open, no login needed.
  auth_required: boolean;
  authenticated: boolean;
  user?: { email: string; name: string };
};

const ME_KEY = "/auth/me";
const SIGNED_OUT: Me = { auth_required: true, authenticated: false };

/// `/auth/me` never surfaces an error to the UI: a network failure or non-200
/// just means "treat as signed out".
async function meFetcher(key: string): Promise<Me> {
  try {
    const res = await fetch(key, { credentials: "same-origin" });
    if (!res.ok) return SIGNED_OUT;
    return (await res.json()) as Me;
  } catch {
    return SIGNED_OUT;
  }
}

/// The current session. Re-checks on window focus so a lapsed session bounces
/// back to login when the operator returns to the tab.
export function useMe(): { me: Me | undefined; isLoading: boolean } {
  const { data, isLoading } = useSWR<Me>(ME_KEY, meFetcher, {
    revalidateOnFocus: true,
  });
  return { me: data, isLoading };
}

/// Redirect the browser into the Lark OAuth flow.
export function login(): void {
  window.location.assign("/auth/login");
}

/// End the session, then re-probe `/auth/me` so the UI returns to login.
export async function logout(): Promise<void> {
  await fetch("/auth/logout", { method: "POST", credentials: "same-origin" });
  await mutate(ME_KEY);
}

/// EventSource sends the session cookie on same-origin requests, so the SSE URL
/// needs no token.
export function sseUrl(path: string): string {
  return path;
}
