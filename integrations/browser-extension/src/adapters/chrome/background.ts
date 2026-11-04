// Chrome MV3 service worker: the composition root for the background.

import type { WebExtApi } from "../webext/api.js";
import { startBackground } from "../webext/background.js";
import { chromeCookieQueries } from "./cookies.js";

const api = (globalThis as unknown as { chrome: WebExtApi }).chrome;
startBackground(api, { cookieQueries: chromeCookieQueries, consentGesture: "button", language: navigator.language });
