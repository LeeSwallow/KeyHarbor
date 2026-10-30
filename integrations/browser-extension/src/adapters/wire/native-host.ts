// NativeHost over any transport that carries one protocol-v1 message and returns the raw reply.
// Translates our requests to the wire and the replies back; it decides nothing.

import type { NativeHost } from "../../ports/index.js";
import {
  type HostRequestWire,
  parseFillReply,
  parseHostReply,
  parseLoginsReply,
  parseSaveReply,
  toAutofillFillWire,
  toAutofillListWire,
  toReceiveSiteWire,
  toSaveLoginWire,
} from "./protocol.js";

/** Sends one message; rejects when the host is unavailable. */
export type HostTransport = (message: HostRequestWire) => Promise<unknown>;

export function nativeHostOver(post: HostTransport): NativeHost {
  return {
    receiveSite: async (handoff) => parseHostReply(await post(toReceiveSiteWire(handoff))),
    listLogins: async (origin) => parseLoginsReply(await post(toAutofillListWire(origin))),
    fillLogin: async (origin, item) => parseFillReply(await post(toAutofillFillWire(origin, item))),
    saveLogin: async (origin, login) => parseSaveReply(await post(toSaveLoginWire(origin, login))),
  };
}
