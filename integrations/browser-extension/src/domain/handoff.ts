// What a person asks for when they hand a site over, checked when it is made.

import { type Result, err, ok } from "./result.js";

/** Grant lengths the popup offers, and the bounds any request must respect (R-43). */
export const TTL_CHOICES = [3600, 8 * 3600, 24 * 3600] as const;
export const TTL_RANGE = { min: 60, max: 7 * 24 * 3600 } as const;

const SESSION_NAME = /^[a-z0-9][a-z0-9_-]{0,63}$/;

export interface HandOffRequest {
  session: string;
  ttlSeconds: number;
  includeLocalStorage: boolean;
}

/** A request with a valid session name and TTL, or the first thing wrong with it. */
export function handOffRequest(session: string, ttlSeconds: number, includeLocalStorage: boolean): Result<HandOffRequest> {
  if (!SESSION_NAME.test(session)) return err("bad_session");
  if (!Number.isInteger(ttlSeconds) || ttlSeconds < TTL_RANGE.min || ttlSeconds > TTL_RANGE.max) return err("bad_ttl");
  return ok({ session, ttlSeconds, includeLocalStorage });
}
