// The bridge to the KeyHarbor app (spec 2026-10-31): a lasting native connection over which the
// host passes on the app's asks (`request_site`); each is answered by the site-request use case.
// While the connection is open Chrome keeps the service worker alive; when it drops (the host is
// not installed, KeyHarbor was updated) the bridge tries again a minute later.

import type { SiteRequestOutcome } from "../../domain/site-request.js";
import { HOST_NAME, parseRequestSite, toBridgeWire, toRequestAnswerWire } from "../wire/protocol.js";
import type { NativePortWire, WebExtApi } from "./api.js";

/**
 * How long to wait before connecting again. Fixed, and not tied to the app's BRIDGE_WAIT: a
 * missing host fails at once, so a shorter wait only spins, and a longer one leaves a freshly
 * installed host unreached for longer.
 */
const RETRY_MS = 60_000;

export type AnswerSiteRequest = (origin: string, session: string) => Promise<SiteRequestOutcome>;

/** Posts on a port that may have dropped meanwhile; the reconnect is onDisconnect's job. */
function postQuietly(port: NativePortWire, message: unknown): void {
  try {
    port.postMessage(message);
  } catch {
    // The port is gone; the host sees the drop and ends the ask without an answer.
  }
}

export function startBridge(api: WebExtApi, answer: AnswerSiteRequest, retryMs: number = RETRY_MS): void {
  const connect = () => {
    let port: NativePortWire;
    try {
      port = api.runtime.connectNative(HOST_NAME);
    } catch {
      setTimeout(connect, retryMs);
      return;
    }
    port.onMessage.addListener((raw) => {
      const ask = parseRequestSite(raw);
      if (!ask) return;
      void answer(ask.origin, ask.session).then(
        (outcome) => postQuietly(port, toRequestAnswerWire(ask.id, outcome)),
        () => postQuietly(port, toRequestAnswerWire(ask.id, { outcome: "failed" })),
      );
    });
    port.onDisconnect.addListener(() => setTimeout(connect, retryMs));
    port.postMessage(toBridgeWire());
  };
  connect();
}
