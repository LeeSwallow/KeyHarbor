// The per-browser composition roots, run against a fake namespace on globalThis.

import { afterEach, describe, expect, it, vi } from "vitest";
import { HOST_NAME } from "../../src/domain/protocol.js";
import { fakeBrowser } from "../support/fake-browser.js";

const cookie = { name: "s", value: "v", domain: ".github.com", hostOnly: false, path: "/", secure: true, httpOnly: true, sameSite: "lax", session: true };
const command = { type: "hand_off", tabId: 7, session: "work", ttlSeconds: 3600, includeLocalStorage: false };

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

describe.each([
  ["chrome", "chrome", "../../src/adapters/chrome/background.js"],
  ["firefox", "browser", "../../src/adapters/firefox/background.js"],
] as const)("%s background", (_browser, global, entry) => {
  it("answers a hand_off command through the native host", async () => {
    const api = fakeBrowser({ cookies: [cookie] });
    vi.stubGlobal(global, api);
    await import(entry);
    expect(api.listeners).toHaveLength(1);
    const reply = await api.deliver(command);
    expect(reply).toEqual({ ok: true, value: { session: "work", origin: "https://github.com", cookies: 1, localStorage: false } });
    expect(api.calls.native).toHaveLength(1);
    expect(api.calls.native[0]?.app).toBe(HOST_NAME);
  });
});

describe("cookie queries", () => {
  it("chrome reads unpartitioned and this site's CHIPS cookies", async () => {
    const api = fakeBrowser({ cookies: [cookie] });
    vi.stubGlobal("chrome", api);
    await import("../../src/adapters/chrome/background.js");
    await api.deliver(command);
    expect(api.calls.getAll).toEqual([{ domain: "github.com" }, { domain: "github.com", partitionKey: { topLevelSite: "https://github.com" } }]);
  });

  it("firefox reads every partition and first-party domain in one query", async () => {
    const api = fakeBrowser({ cookies: [cookie] });
    vi.stubGlobal("browser", api);
    await import("../../src/adapters/firefox/background.js");
    await api.deliver(command);
    expect(api.calls.getAll).toEqual([{ domain: "github.com", partitionKey: {}, firstPartyDomain: null }]);
  });
});
