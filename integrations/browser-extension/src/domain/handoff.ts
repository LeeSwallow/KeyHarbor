// What a person asks for when they hand a site over, checked when it is made; what goes to the
// KeyHarbor host; and what it answers. All in our words: the Native Messaging shape is the
// adapter's (adapters/wire/protocol.ts).

import type { SiteCookie } from "./cookie.js";
import { type Result, err, ok } from "./result.js";
import { type SessionName, parseSessionName } from "./session-name.js";

/** Grant lengths the popup offers, and the bounds any request must respect (R-43). */
export const TTL_CHOICES = [3600, 8 * 3600, 24 * 3600] as const;
export const TTL_RANGE = { min: 60, max: 7 * 24 * 3600 } as const;

export interface HandOffRequest {
  session: SessionName;
  ttlSeconds: number;
  includeLocalStorage: boolean;
}

/** A request with a valid session name and TTL, or the first thing wrong with it. */
export function handOffRequest(session: string, ttlSeconds: number, includeLocalStorage: boolean): Result<HandOffRequest> {
  const name = parseSessionName(session);
  if (!name.ok) return name;
  if (!Number.isInteger(ttlSeconds) || ttlSeconds < TTL_RANGE.min || ttlSeconds > TTL_RANGE.max) return err("bad_ttl");
  return ok({ session: name.value, ttlSeconds, includeLocalStorage });
}

/** One site's login on its way to an agent session. Values travel only inside this. */
export interface SiteHandoff {
  /** The tab's origin, e.g. "https://github.com". The host opens it after setting cookies. */
  origin: string;
  /** The cookies that go over (`selectSiteCookies`). */
  cookies: SiteCookie[];
  session: SessionName;
  ttlSeconds: number;
  /** That origin's localStorage, only when the person ticked it. */
  localStorage?: Record<string, string>;
}

/** What the host answers. Neither form carries a cookie value. */
export type HostReply = { kind: "received"; session: string; cookies: number } | { kind: "refused"; code: string };
