// TabInfo over `tabs`.

import type { Tab, TabInfo } from "../../ports/index.js";
import type { TabWire, WebExtApi } from "./api.js";

const toTab = (t: TabWire | undefined): Tab | null => (t && typeof t.id === "number" && typeof t.url === "string" ? { id: t.id, url: t.url } : null);

export function webextTabInfo(api: WebExtApi): TabInfo {
  return {
    active: async () => toTab((await api.tabs.query({ active: true, currentWindow: true }))[0]),
    find: (tabId) => api.tabs.get(tabId).then(toTab, () => null),
  };
}
