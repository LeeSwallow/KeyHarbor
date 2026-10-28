// NativeHost over any transport that carries one protocol-v1 message and returns the raw reply.
// Translates our hand-off to `receive_site` and the reply back; it decides nothing.

import type { NativeHost } from "../../ports/index.js";
import { type HostRequestWire, parseHostReply, toReceiveSiteWire } from "./protocol.js";

/** Sends one message; rejects when the host is unavailable. */
export type HostTransport = (message: HostRequestWire) => Promise<unknown>;

export function nativeHostOver(post: HostTransport): NativeHost {
  return { receiveSite: async (handoff) => parseHostReply(await post(toReceiveSiteWire(handoff))) };
}
