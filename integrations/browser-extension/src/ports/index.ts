// The ports the use cases need. Adapters (adapters/<browser>/) implement them; tests fake them.
export type { CookieSource } from "./cookie-source.js";
export type { StorageSource } from "./storage-source.js";
export type { NativeHost } from "./native-host.js";
export type { Tab, TabInfo } from "./tab-info.js";

import type { CookieSource } from "./cookie-source.js";
import type { NativeHost } from "./native-host.js";
import type { StorageSource } from "./storage-source.js";
import type { TabInfo } from "./tab-info.js";

/** Everything the hand-off use case is given. */
export interface Ports {
  cookies: CookieSource;
  storage: StorageSource;
  host: NativeHost;
  tabs: TabInfo;
}
