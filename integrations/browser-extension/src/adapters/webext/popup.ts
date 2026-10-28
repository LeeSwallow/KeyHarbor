// The popup's dependencies, from one browser namespace.

import type { PopupDeps } from "../../ui/popup/deps.js";
import type { WebExtApi } from "./api.js";
import { webextTabInfo } from "./tabs.js";

const LAST_SESSION = "lastSession";

export function webextPopupDeps(api: WebExtApi, language: string): PopupDeps {
  const tabs = webextTabInfo(api);
  return {
    language,
    activeTab: () => tabs.active(),
    requestHostAccess: (origins) => api.permissions.request({ origins }),
    send: (command) => api.runtime.sendMessage(command) as ReturnType<PopupDeps["send"]>,
    loadSession: async () => {
      const v = (await api.storage.local.get(LAST_SESSION))[LAST_SESSION];
      return typeof v === "string" ? v : null;
    },
    saveSession: (session) => api.storage.local.set({ [LAST_SESSION]: session }),
  };
}
