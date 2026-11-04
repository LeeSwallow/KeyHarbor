// Firefox popup entry: the composition root for the popup.

import type { WebExtApi } from "../webext/api.js";
import { startPopup } from "../webext/popup.js";

const api = (globalThis as unknown as { browser: WebExtApi }).browser;
void startPopup(api, document, navigator.language);
