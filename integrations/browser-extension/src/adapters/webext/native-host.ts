// NativeHost over `runtime.sendNativeMessage`. One request, one reply.

import { HOST_NAME } from "../../domain/protocol.js";
import type { NativeHost } from "../../ports/index.js";
import type { WebExtApi } from "./api.js";

export function webextNativeHost(api: WebExtApi, hostName: string = HOST_NAME): NativeHost {
  return { send: (request) => api.runtime.sendNativeMessage(hostName, request) };
}
