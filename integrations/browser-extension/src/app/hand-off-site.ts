// Use case: hand the site of one tab to an agent session (R-42). Reads that site's cookies (and,
// if asked, the page's localStorage), sends exactly one `receive_site` message to the native
// host, and reports counts. It never logs and its errors are codes: no value leaves through here
// except inside the one message to the host.

import { toCookieParams } from "../domain/cookie.js";
import type { HandOffRequest } from "../domain/handoff.js";
import { parseHostReply, toReceiveSiteWire } from "../domain/protocol.js";
import { type Result, err, ok } from "../domain/result.js";
import { scopeOf } from "../domain/scope.js";
import type { Ports } from "../ports/index.js";

export interface HandOffOutcome {
  session: string;
  origin: string;
  /** How many cookies the host set. */
  cookies: number;
  localStorage: boolean;
}

export type HandOffSite = (tabId: number, request: HandOffRequest) => Promise<Result<HandOffOutcome>>;

export function makeHandOffSite(ports: Ports): HandOffSite {
  return async function handOffSite(tabId, request) {
    const tab = await ports.tabs.find(tabId);
    if (!tab) return err("unsupported_page");
    const scope = scopeOf(tab.url);
    if (!scope.ok) return scope;

    const cookies = await ports.cookies.readSiteCookies(scope.value).catch(() => null);
    if (cookies === null) return err("no_permission");
    const { params } = toCookieParams(cookies, scope.value);
    if (params.length === 0) return err("no_cookies");

    let localStorage: Record<string, string> | undefined;
    if (request.includeLocalStorage) {
      localStorage = await ports.storage.readLocalStorage(tab.id).catch(() => undefined);
      if (!localStorage) return err("storage_unavailable");
    }

    const message = toReceiveSiteWire(scope.value.origin, params, request, localStorage);
    const raw = await ports.host.send(message).then(
      (r) => ({ sent: true as const, r }),
      () => ({ sent: false as const }),
    );
    if (!raw.sent) return err("host_unavailable");
    const reply = parseHostReply(raw.r);
    if (!reply.ok) return reply;
    if (reply.value.kind === "refused") return err("host_refused");
    return ok({ session: reply.value.session, origin: scope.value.origin, cookies: reply.value.cookies, localStorage: localStorage !== undefined });
  };
}
