// The popup's dependencies, from one browser namespace.

import type { PopupDeps } from "../../ui/popup/deps.js";
import { toHandOffCommandWire } from "../wire/command.js";
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
    loadSession: async () => {
      const v = (await api.storage.local.get(LAST_SESSION))[LAST_SESSION];
      return typeof v === "string" ? v : null;
    },
    saveSession: (session) => api.storage.local.set({ [LAST_SESSION]: session }),
  };
}
