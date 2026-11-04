// Firefox MV3 background script: the composition root for the background.

import type { WebExtApi } from "../webext/api.js";
import { startBackground } from "../webext/background.js";
import { firefoxCookieQueries } from "./cookies.js";

const api = (globalThis as unknown as { browser: WebExtApi }).browser;
startBackground(api, { cookieQueries: firefoxCookieQueries, consentGesture: "click", language: navigator.language });
