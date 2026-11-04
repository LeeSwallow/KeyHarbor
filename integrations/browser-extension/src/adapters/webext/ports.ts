// Builds every port from one browser namespace. Browser adapters pass what differs between them
// (their cookie queries, how a notification is answered) and navigator.language.

import type { Ports } from "../../ports/index.js";
import { pickLanguage } from "../../ui/popup/i18n.js";
import type { WebExtApi } from "./api.js";
import { webextNotifier, webextPermissions } from "./sources/consent.js";
import type { ConsentGesture } from "./sources/consent-text.js";
import { type CookieQueries, webextCookieSource } from "./sources/cookies.js";
import { webextNativeHost } from "./sources/native-host.js";
import { webextPageForm } from "./sources/page-form.js";
import { webextStorageSource } from "./sources/storage.js";
import { webextTabInfo } from "./tabs.js";

export interface BrowserTraits {
  cookieQueries: CookieQueries;
  consentGesture: ConsentGesture;
  /** navigator.language: Korean notifications for "ko*", English otherwise. */
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
    notifier: webextNotifier(api, { gesture: traits.consentGesture, lang: pickLanguage(traits.language) }),
  };
}
