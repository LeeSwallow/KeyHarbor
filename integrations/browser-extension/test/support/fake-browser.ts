// A fake WebExtension namespace, recording calls. Used as globalThis.chrome or globalThis.browser.

import type { CookieWire, MessageListener, WebExtApi } from "../../src/adapters/webext/api.js";

export interface FakeBrowser extends WebExtApi {
  calls: { getAll: Array<Record<string, unknown>>; native: Array<{ app: string; message: unknown }>; requested: string[][] };
  listeners: MessageListener[];
  /** Delivers a runtime message and resolves with the listener's response. */
  deliver(message: unknown): Promise<unknown>;
}

export function fakeBrowser(opts: { cookies?: CookieWire[]; tabUrl?: string; reply?: unknown } = {}): FakeBrowser {
  const calls: FakeBrowser["calls"] = { getAll: [], native: [], requested: [] };
  const listeners: MessageListener[] = [];
  const store: Record<string, unknown> = {};
  const tab = { id: 7, url: opts.tabUrl ?? "https://github.com/" };
  return {
    calls,
    listeners,
    cookies: {
      getAll: async (details) => {
        calls.getAll.push(details);
        return opts.cookies ?? [];
      },
    },
    tabs: { query: async () => [tab], get: async (id) => (id === tab.id ? tab : Promise.reject(new Error("no tab"))) },
    scripting: { executeScript: async () => [{ result: { token: "t" } }] },
    runtime: {
      sendNativeMessage: async (app, message) => {
        calls.native.push({ app, message });
        return opts.reply ?? { v: 1, type: "received", session: "work", cookies: 1 };
      },
      sendMessage: async (m) => m,
      onMessage: { addListener: (l) => void listeners.push(l) },
      connectNative: () => ({ postMessage: () => undefined, onMessage: { addListener: () => undefined }, onDisconnect: { addListener: () => undefined } }),
      getURL: (p) => `chrome-extension://test/${p}`,
    },
    permissions: {
      request: async ({ origins }) => {
        calls.requested.push(origins);
        return true;
      },
      contains: async () => true,
    },
    notifications: {
      create: async (id) => id,
      clear: async () => true,
      onClicked: { addListener: () => undefined },
      onButtonClicked: { addListener: () => undefined },
      onClosed: { addListener: () => undefined },
    },
    storage: {
      local: {
        get: async (k) => ({ [k]: store[k] }),
        set: async (items) => void Object.assign(store, items),
      },
    },
    deliver(message) {
      return new Promise((resolve) => {
        const keepOpen = listeners[0]?.(message, {}, resolve);
        if (keepOpen !== true) resolve(undefined);
      });
    },
  };
}
