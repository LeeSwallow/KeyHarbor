// The Native Messaging protocol between the extension and the KeyHarbor native host (phase 4).
// Wire types end in `Wire`; this file is the only place that builds or reads them.
// Version 1: one request (`receive_site`) and its reply.

import type { CdpCookieParam } from "./cookie.js";
import { type Result, err, ok } from "./result.js";

export const PROTOCOL_VERSION = 1;

/** The native host's registered name (native-host/*.json). */
export const HOST_NAME = "dev.keyharbor.host";

/** Hand one site's login to an agent session. Values travel only inside this message. */
export interface ReceiveSiteWire {
  v: typeof PROTOCOL_VERSION;
  type: "receive_site";
  /** The tab's origin, e.g. "https://github.com". The host opens it after setting cookies. */
  origin: string;
  /** Cookies of the registrable domain and its subdomains, as CDP `Network.CookieParam`. */
  cookies: CdpCookieParam[];
  /** That origin's localStorage, only when the person ticked it. */
  local_storage?: Record<string, string>;
  /** How long the grant for this site lasts (R-43). */
  ttl_seconds: number;
  /** The agent session that receives the login. */
  session: string;
}

/** Every message the extension sends. A new request is a new member here. */
export type HostRequestWire = ReceiveSiteWire;

/** What the host answers. Neither form carries a cookie value. */
export type HostReplyWire =
  | { v: typeof PROTOCOL_VERSION; type: "received"; session: string; cookies: number }
  | { v: typeof PROTOCOL_VERSION; type: "error"; code: string };

/** The reply in our words. */
export type HostReply = { kind: "received"; session: string; cookies: number } | { kind: "refused"; code: string };

const isRecord = (x: unknown): x is Record<string, unknown> => typeof x === "object" && x !== null && !Array.isArray(x);

/** Reads a host reply. Anything not shaped like protocol v1 is `bad_reply`. */
export function parseHostReply(raw: unknown): Result<HostReply> {
  if (!isRecord(raw) || raw.v !== PROTOCOL_VERSION) return err("bad_reply");
  if (raw.type === "received" && typeof raw.session === "string" && Number.isInteger(raw.cookies)) {
    return ok({ kind: "received", session: raw.session, cookies: raw.cookies as number });
  }
  if (raw.type === "error" && typeof raw.code === "string" && /^[a-z_]{1,40}$/.test(raw.code)) {
    return ok({ kind: "refused", code: raw.code });
  }
  return err("bad_reply");
}

/** Builds the `receive_site` request. The only place a `ReceiveSiteWire` is made. */
export function toReceiveSiteWire(
  origin: string,
  cookies: CdpCookieParam[],
  request: { session: string; ttlSeconds: number },
  localStorage?: Record<string, string>,
): ReceiveSiteWire {
  const wire: ReceiveSiteWire = { v: PROTOCOL_VERSION, type: "receive_site", origin, cookies, ttl_seconds: request.ttlSeconds, session: request.session };
  if (localStorage) wire.local_storage = localStorage;
  return wire;
}
