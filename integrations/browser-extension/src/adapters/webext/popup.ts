// The popup's dependencies, from one browser namespace. Messages are built and answers read
// in adapters/wire/command.ts only.

import type { PopupDeps } from "../../ui/popup/deps.js";
import {
  type CommandWire,
  parseCommandAnswer,
  toFillLoginWire,
  toHandOffCommandWire,
  toListLoginsWire,
  toSaveLoginWire,
} from "../wire/command.js";
import type { WebExtApi } from "./api.js";
import { webextTabInfo } from "./tabs.js";

const LAST_SESSION = "lastSession";

export function webextPopupDeps(api: WebExtApi, language: string): PopupDeps {
  const tabs = webextTabInfo(api);
  const send = async <W extends CommandWire>(wire: W) => parseCommandAnswer<W["type"]>(wire.type, await api.runtime.sendMessage(wire));
  return {
    language,
    activeTab: () => tabs.active(),
    requestHostAccess: (origins) => api.permissions.request({ origins }),
    handOff: (ask) => send(toHandOffCommandWire(ask)),
    listLogins: (tabId) => send(toListLoginsWire(tabId)),
    fillLogin: (tabId, item) => send(toFillLoginWire(tabId, item)),
    saveLogin: (tabId) => send(toSaveLoginWire(tabId)),
    loadSession: async () => {
      const v = (await api.storage.local.get(LAST_SESSION))[LAST_SESSION];
      return typeof v === "string" ? v : null;
    },
    saveSession: (session) => api.storage.local.set({ [LAST_SESSION]: session }),
  };
}
