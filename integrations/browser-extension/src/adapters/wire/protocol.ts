// The Native Messaging protocol between the extension and the KeyHarbor native host (phase 4).
// Wire types end in `Wire`; this file is the only place that builds or reads them.
// Version 1: `receive_site` (hand a site over) and, from phase 7, `autofill_list`, `autofill_fill`
// and `save_login` (ADR-0017). `filled` is the one reply that carries a value.

import type { HostReply, SiteHandoff } from "../../domain/handoff.js";
import type { LoginChoice, LoginValues, SaveOutcome } from "../../domain/login.js";
import type { SiteRequestOutcome } from "../../domain/site-request.js";
import { type Result, err, ok } from "../../domain/result.js";
import { type CdpCookieParam, toCookieParam } from "./cdp-cookie.js";

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

/** The saved logins that may fill `origin` (names only). */
export interface AutofillListWire {
  v: typeof PROTOCOL_VERSION;
  type: "autofill_list";
  origin: string;
}

/** The values of `item` for `origin`; the host asks Touch ID first. */
export interface AutofillFillWire {
  v: typeof PROTOCOL_VERSION;
  type: "autofill_fill";
  origin: string;
  item: string;
}

/** What the page held when the person pressed save. Values travel only inside this message. */
export interface SaveLoginWire {
  v: typeof PROTOCOL_VERSION;
  type: "save_login";
  origin: string;
  username?: string;
  password: string;
}

/** Every message the extension sends. A new request is a new member here. */
export type HostRequestWire = ReceiveSiteWire | AutofillListWire | AutofillFillWire | SaveLoginWire;

/** What the host answers. Neither form carries a cookie value. */
export type HostReplyWire =
  | { v: typeof PROTOCOL_VERSION; type: "received"; session: string; cookies: number }
  | { v: typeof PROTOCOL_VERSION; type: "error"; code: string };

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
export function toReceiveSiteWire(handoff: SiteHandoff): ReceiveSiteWire {
  const wire: ReceiveSiteWire = {
    v: PROTOCOL_VERSION,
    type: "receive_site",
    origin: handoff.origin,
    cookies: handoff.cookies.map(toCookieParam),
    ttl_seconds: handoff.ttlSeconds,
    session: handoff.session,
  };
  if (handoff.localStorage) wire.local_storage = handoff.localStorage;
  return wire;
}

const hostError = (raw: Record<string, unknown>): Result<never> | null =>
  raw.type === "error" && typeof raw.code === "string" ? err(raw.code === "not_confirmed" ? "not_confirmed" : "host_refused") : null;

export const toAutofillListWire = (origin: string): AutofillListWire => ({ v: PROTOCOL_VERSION, type: "autofill_list", origin });
export const toAutofillFillWire = (origin: string, item: string): AutofillFillWire => ({ v: PROTOCOL_VERSION, type: "autofill_fill", origin, item });
export function toSaveLoginWire(origin: string, login: LoginValues): SaveLoginWire {
  const wire: SaveLoginWire = { v: PROTOCOL_VERSION, type: "save_login", origin, password: login.password };
  if (login.username) wire.username = login.username;
  return wire;
}

/** `logins {items}`: names of logins, nothing else. */
export function parseLoginsReply(raw: unknown): Result<LoginChoice[]> {
  if (!isRecord(raw) || raw.v !== PROTOCOL_VERSION) return err("bad_reply");
  const refused = hostError(raw);
  if (refused) return refused;
  if (raw.type !== "logins" || !Array.isArray(raw.items) || !raw.items.every((i) => typeof i === "string")) return err("bad_reply");
  return ok((raw.items as string[]).map((item) => ({ item })));
}

/** `filled {username?, password}`: the values, to put into the page and drop. */
export function parseFillReply(raw: unknown): Result<LoginValues> {
  if (!isRecord(raw) || raw.v !== PROTOCOL_VERSION) return err("bad_reply");
  const refused = hostError(raw);
  if (refused) return refused;
  if (raw.type !== "filled" || typeof raw.password !== "string") return err("bad_reply");
  const values: LoginValues = { password: raw.password };
  if (typeof raw.username === "string") values.username = raw.username;
  return ok(values);
}

/** `saved {item}` (a draft) or `known {item}`. */
export function parseSaveReply(raw: unknown): Result<SaveOutcome> {
  if (!isRecord(raw) || raw.v !== PROTOCOL_VERSION) return err("bad_reply");
  const refused = hostError(raw);
  if (refused) return refused;
  if ((raw.type !== "saved" && raw.type !== "known") || typeof raw.item !== "string") return err("bad_reply");
  return ok({ kind: raw.type, item: raw.item });
}

/** Opens the lasting connection the app's asks come through (spec 2026-10-31). */
export interface BridgeWire {
  v: typeof PROTOCOL_VERSION;
  type: "bridge";
}
/** The app asks, through the host, to hand `origin` to `session`. */
export interface RequestSiteWire {
  v: typeof PROTOCOL_VERSION;
  type: "request_site";
  id: number;
  session: string;
  origin: string;
}
/** How the ask `id` ended. A code and a count; no value. */
export type RequestAnswerWire = { v: typeof PROTOCOL_VERSION; type: "request_answer"; id: number } & SiteRequestOutcome;

export const toBridgeWire = (): BridgeWire => ({ v: PROTOCOL_VERSION, type: "bridge" });
export const toRequestAnswerWire = (id: number, outcome: SiteRequestOutcome): RequestAnswerWire => ({ v: PROTOCOL_VERSION, type: "request_answer", id, ...outcome });

/** A `request_site` from the host, or null for anything else. */
export function parseRequestSite(raw: unknown): { id: number; session: string; origin: string } | null {
  if (!isRecord(raw) || raw.v !== PROTOCOL_VERSION || raw.type !== "request_site") return null;
  if (!Number.isInteger(raw.id) || typeof raw.session !== "string" || typeof raw.origin !== "string") return null;
  return { id: raw.id as number, session: raw.session, origin: raw.origin };
}
