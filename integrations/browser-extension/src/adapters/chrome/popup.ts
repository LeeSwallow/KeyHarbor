// Chrome popup entry: the composition root for the popup.

import { mountPopup } from "../../ui/popup/popup.js";
import type { WebExtApi } from "../webext/api.js";
import { webextPopupDeps } from "../webext/popup.js";

const api = (globalThis as unknown as { chrome: WebExtApi }).chrome;
void mountPopup(document, webextPopupDeps(api, navigator.language));
