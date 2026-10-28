// NativeHost over `runtime.sendNativeMessage`. One request, one reply.

import type { NativeHost } from "../../ports/index.js";
import { nativeHostOver } from "../wire/native-host.js";
import { HOST_NAME } from "../wire/protocol.js";
import type { WebExtApi } from "./api.js";

export function webextNativeHost(api: WebExtApi, hostName: string = HOST_NAME): NativeHost {
  return nativeHostOver((message) => api.runtime.sendNativeMessage(hostName, message));
}
