// The bridge to the KeyHarbor app (spec 2026-10-31): a lasting native connection over which the
// host passes on the app's asks (`request_site`); each is answered by the site-request use case.
// While the connection is open Chrome keeps the service worker alive; when it drops (the host is
// not installed, KeyHarbor was updated) the bridge tries again a minute later.

import { parseRequestSite, toBridgeWire, toRequestAnswerWire } from "../wire/protocol.js";
import { HOST_NAME } from "../wire/protocol.js";
import type { SiteRequestOutcome } from "../../domain/site-request.js";
import type { WebExtApi } from "./api.js";

const RETRY_MS = 60_000;

export type AnswerSiteRequest = (origin: string, session: string) => Promise<SiteRequestOutcome>;

export function startBridge(api: WebExtApi, answer: AnswerSiteRequest, retryMs: number = RETRY_MS): void {
  const connect = () => {
    let port;
    try {
      port = api.runtime.connectNative(HOST_NAME);
    } catch {
      setTimeout(connect, retryMs);
      return;
    }
    const live = port;
    live.onMessage.addListener((raw) => {
      const ask = parseRequestSite(raw);
      if (!ask) return;
      void answer(ask.origin, ask.session).then(
        (outcome) => live.postMessage(toRequestAnswerWire(ask.id, outcome)),
        () => live.postMessage(toRequestAnswerWire(ask.id, { outcome: "failed" })),
      );
    });
    live.onDisconnect.addListener(() => setTimeout(connect, retryMs));
    live.postMessage(toBridgeWire());
  };
  connect();
}
