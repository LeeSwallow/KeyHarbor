// Chrome MV3 service worker: the composition root for the background.

import { makeCommandHandler } from "../../app/background.js";
import { makeAnswerSiteRequest } from "../../app/site-request.js";
import type { WebExtApi } from "../webext/api.js";
import { listenForCommands } from "../webext/background.js";
import { startBridge } from "../webext/bridge.js";
import { webextPorts } from "../webext/ports.js";
import { chromeCookieQueries } from "./cookies.js";

const api = (globalThis as unknown as { chrome: WebExtApi }).chrome;
const ports = webextPorts(api, { cookieQueries: chromeCookieQueries, notificationButtons: true, language: navigator.language });
listenForCommands(api, makeCommandHandler(ports));
// The KeyHarbor app's asks to hand a dragged site over (spec 2026-10-31).
startBridge(api, makeAnswerSiteRequest(ports));
