// StorageSource: runs a one-line reader in the tab (needs the `scripting` permission and host access).

import type { StorageSource } from "../../ports/index.js";
import type { WebExtApi } from "./api.js";

/** Runs inside the page. Must be self-contained: it is serialised, not closed over. */
export function readPageLocalStorage(): Record<string, string> {
  const out: Record<string, string> = {};
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key !== null) out[key] = localStorage.getItem(key) ?? "";
  }
  return out;
}

export function webextStorageSource(api: WebExtApi): StorageSource {
  return {
    async readLocalStorage(tabId) {
      const [frame] = await api.scripting.executeScript({ target: { tabId }, func: readPageLocalStorage });
      const result = frame?.result;
      if (typeof result !== "object" || result === null) throw new Error("storage_unavailable");
      return Object.fromEntries(Object.entries(result).filter((e): e is [string, string] => typeof e[1] === "string"));
    },
  };
}
