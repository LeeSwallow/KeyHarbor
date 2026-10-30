// Firefox MV3 background script: the composition root for the background.

import { makeCommandHandler } from "../../app/background.js";
import { makeAnswerSiteRequest } from "../../app/site-request.js";
import type { WebExtApi } from "../webext/api.js";
import { listenForCommands } from "../webext/background.js";
import { startBridge } from "../webext/bridge.js";
import { webextPorts } from "../webext/ports.js";
import { firefoxCookieQueries } from "./cookies.js";

const api = (globalThis as unknown as { browser: WebExtApi }).browser;
const ports = webextPorts(api, { cookieQueries: firefoxCookieQueries, notificationButtons: false, language: navigator.language });
listenForCommands(api, makeCommandHandler(ports));
// The KeyHarbor app's asks to hand a dragged site over (spec 2026-10-31).
startBridge(api, makeAnswerSiteRequest(ports));
