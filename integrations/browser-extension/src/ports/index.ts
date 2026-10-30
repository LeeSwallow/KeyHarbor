// The ports the use cases need. Adapters (adapters/<browser>/) implement them; tests fake them.
export type { CookieSource } from "./cookie-source.js";
export type { StorageSource } from "./storage-source.js";
export type { NativeHost } from "./native-host.js";
export type { PageForm } from "./page-form.js";
export type { Notifier, Permissions } from "./consent.js";
export type { Tab, TabInfo } from "./tab-info.js";

import type { CookieSource } from "./cookie-source.js";
import type { NativeHost } from "./native-host.js";
import type { PageForm } from "./page-form.js";
import type { Notifier, Permissions } from "./consent.js";
import type { StorageSource } from "./storage-source.js";
import type { TabInfo } from "./tab-info.js";

/** Everything the use cases are given. */
export interface Ports {
  cookies: CookieSource;
  storage: StorageSource;
  host: NativeHost;
  tabs: TabInfo;
  page: PageForm;
  permissions: Permissions;
  notifier: Notifier;
}
