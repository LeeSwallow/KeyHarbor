// Chrome MV3 service worker: the composition root for the background.

import { makeCommandHandler } from "../../app/background.js";
import type { WebExtApi } from "../webext/api.js";
import { listenForCommands } from "../webext/background.js";
import { webextPorts } from "../webext/ports.js";
import { chromeCookieQueries } from "./cookies.js";

const api = (globalThis as unknown as { chrome: WebExtApi }).chrome;
listenForCommands(api, makeCommandHandler(webextPorts(api, chromeCookieQueries)));
