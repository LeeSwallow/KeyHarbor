// Builds every port from one browser namespace. Browser adapters call this with their queries.

import type { Ports } from "../../ports/index.js";
import type { WebExtApi } from "./api.js";
import { type CookieQueries, webextCookieSource } from "./cookies.js";
import { webextNativeHost } from "./native-host.js";
import { webextStorageSource } from "./storage.js";
import { webextTabInfo } from "./tabs.js";

export function webextPorts(api: WebExtApi, cookieQueries: CookieQueries): Ports {
  return {
    cookies: webextCookieSource(api, cookieQueries),
    storage: webextStorageSource(api),
    host: webextNativeHost(api),
    tabs: webextTabInfo(api),
  };
}
