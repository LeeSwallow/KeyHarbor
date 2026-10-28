// Firefox MV3 background script: the composition root for the background.

import { makeCommandHandler } from "../../app/background.js";
import type { WebExtApi } from "../webext/api.js";
import { listenForCommands } from "../webext/background.js";
import { webextPorts } from "../webext/ports.js";
import { firefoxCookieQueries } from "./cookies.js";

const api = (globalThis as unknown as { browser: WebExtApi }).browser;
listenForCommands(api, makeCommandHandler(webextPorts(api, firefoxCookieQueries)));
