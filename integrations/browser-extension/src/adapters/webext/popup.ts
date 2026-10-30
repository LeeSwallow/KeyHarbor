// The popup's dependencies, from one browser namespace.

import type { PopupDeps } from "../../ui/popup/deps.js";
import { type LoginCommandWire, toHandOffCommandWire } from "../wire/command.js";
import type { WebExtApi } from "./api.js";
import { webextTabInfo } from "./tabs.js";

const LAST_SESSION = "lastSession";

export function webextPopupDeps(api: WebExtApi, language: string): PopupDeps {
  const tabs = webextTabInfo(api);
  return {
    language,
    activeTab: () => tabs.active(),
    requestHostAccess: (origins) => api.permissions.request({ origins }),
    handOff: (ask) => api.runtime.sendMessage(toHandOffCommandWire(ask)) as ReturnType<PopupDeps["handOff"]>,
    listLogins: (tabId) => api.runtime.sendMessage({ type: "list_logins", tabId } satisfies LoginCommandWire) as ReturnType<PopupDeps["listLogins"]>,
    fillLogin: (tabId, item) => api.runtime.sendMessage({ type: "fill_login", tabId, item } satisfies LoginCommandWire) as ReturnType<PopupDeps["fillLogin"]>,
    saveLogin: (tabId) => api.runtime.sendMessage({ type: "save_login", tabId } satisfies LoginCommandWire) as ReturnType<PopupDeps["saveLogin"]>,
    loadSession: async () => {
      const v = (await api.storage.local.get(LAST_SESSION))[LAST_SESSION];
      return typeof v === "string" ? v : null;
    },
    saveSession: (session) => api.storage.local.set({ [LAST_SESSION]: session }),
  };
}
