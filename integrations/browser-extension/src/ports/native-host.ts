import type { HostRequestWire } from "../domain/protocol.js";

/** The KeyHarbor native host, over Native Messaging. */
export interface NativeHost {
  /** Sends one request and resolves with the host's raw reply. Rejects when the host is unavailable. */
  send(request: HostRequestWire): Promise<unknown>;
}
