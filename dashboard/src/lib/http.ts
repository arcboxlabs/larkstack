import { mutate } from "swr";
import type { SWRConfiguration } from "swr";

/// Error thrown by the fetchers below for any non-2xx response. Carries the
/// status (so callers / the global handler can branch on 401) and the parsed
/// body (`{ "error": "..." }` when the server sent one).
export class HttpError extends Error {
  readonly status: number;
  readonly info: unknown;
  constructor(status: number, info: unknown, message: string) {
    super(message);
    this.name = "HttpError";
    this.status = status;
    this.info = info;
  }
}

/// Best-effort `unknown` → string for error UI.
export function errMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/// Build an `HttpError` from a failed response, preferring the server's
/// `{ "error": "..." }` message, then a raw text body, then the status line.
async function errorFrom(res: Response): Promise<HttpError> {
  let info: unknown = null;
  let message = `${res.status} ${res.statusText}`.trim();
  try {
    const text = await res.text();
    if (text) {
      try {
        info = JSON.parse(text);
        const maybe = (info as { error?: unknown })?.error;
        if (typeof maybe === "string") message = maybe;
      } catch {
        info = text;
        message = text;
      }
    }
  } catch {
    // body unavailable — keep the status-line message
  }
  return new HttpError(res.status, info, message);
}

/// Default SWR fetcher: GET → JSON, throws `HttpError` on failure. The session
/// rides in the cookie (`credentials: "same-origin"`), so no auth header.
export async function jsonFetcher<T>(key: string): Promise<T> {
  const res = await fetch(key, { credentials: "same-origin" });
  if (!res.ok) throw await errorFrom(res);
  return (await res.json()) as T;
}

/// Per-key fetcher override for `/api/config`, which returns raw TOML text.
export async function textFetcher(key: string): Promise<string> {
  const res = await fetch(key, { credentials: "same-origin" });
  if (!res.ok) throw await errorFrom(res);
  return res.text();
}

export interface MutateInit {
  method?: "POST" | "PUT" | "DELETE";
  /// JSON body (sets `Content-Type: application/json`). `null` is sent verbatim.
  json?: unknown;
  /// Raw body, paired with `contentType` (e.g. the TOML config PUT).
  body?: BodyInit;
  contentType?: string;
}

/// The console's single write primitive, shaped for `useSWRMutation`'s
/// `(key, { arg })` fetcher. Returns the parsed JSON body (or `null` when empty)
/// and throws `HttpError` on any non-2xx — note 2xx covers `202` (fire-and-
/// forget action dispatch), and the lark-app credential test answers `200`
/// `{ ok:false }` on bad creds, which is therefore NOT an error.
export async function mutateRequest<T>(
  url: string,
  init: MutateInit = {},
): Promise<T | null> {
  const headers = new Headers();
  let body: BodyInit | undefined;
  if (init.json !== undefined) {
    headers.set("Content-Type", "application/json");
    body = JSON.stringify(init.json);
  } else if (init.body !== undefined) {
    if (init.contentType) headers.set("Content-Type", init.contentType);
    body = init.body;
  }
  const res = await fetch(url, {
    method: init.method ?? "POST",
    credentials: "same-origin",
    headers,
    body,
  });
  if (!res.ok) throw await errorFrom(res);
  const text = await res.text();
  return text ? (JSON.parse(text) as T) : null;
}

/// Global SWR config: JSON fetcher by default, no refetch-on-focus or silent
/// retries (single-operator admin console — surface errors instead), and a 401
/// re-probes the session so the UI drops to the login screen.
export const swrConfig: SWRConfiguration = {
  fetcher: jsonFetcher,
  revalidateOnFocus: false,
  shouldRetryOnError: false,
  onError: (err) => {
    if (err instanceof HttpError && err.status === 401) {
      void mutate("/auth/me");
    }
  },
};
