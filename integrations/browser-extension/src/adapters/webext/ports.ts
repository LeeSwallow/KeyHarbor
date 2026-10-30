// Builds every port from one browser namespace. Browser adapters call this with their queries and
// whether their notifications have buttons.

import type { Ports } from "../../ports/index.js";
import type { WebExtApi } from "./api.js";
import { webextNotifier, webextPermissions } from "./consent.js";
import { type CookieQueries, webextCookieSource } from "./cookies.js";
import { webextNativeHost } from "./native-host.js";
import { webextPageForm } from "./page-form.js";
import { webextStorageSource } from "./storage.js";
import { webextTabInfo } from "./tabs.js";

export interface BrowserTraits {
  cookieQueries: CookieQueries;
  /** Chrome's notifications have buttons; Firefox's do not. */
  notificationButtons: boolean;
  language: string;
}

export function webextPorts(api: WebExtApi, traits: BrowserTraits): Ports {
  return {
    cookies: webextCookieSource(api, traits.cookieQueries),
    storage: webextStorageSource(api),
    host: webextNativeHost(api),
    tabs: webextTabInfo(api),
    page: webextPageForm(api),
    permissions: webextPermissions(api),
    notifier: webextNotifier(api, { buttons: traits.notificationButtons, korean: traits.language.toLowerCase().startsWith("ko") }),
  };
}
