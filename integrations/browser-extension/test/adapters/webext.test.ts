import { describe, expect, it } from "vitest";
import { toSiteCookie, webextCookieSource } from "../../src/adapters/webext/cookies.js";
import { webextPopupDeps } from "../../src/adapters/webext/popup.js";
import { webextStorageSource } from "../../src/adapters/webext/storage.js";
import { scopeOf } from "../../src/domain/scope.js";
import { fakeBrowser } from "../support/fake-browser.js";

const wire = { name: "a", value: "v", domain: ".github.com", hostOnly: false, path: "/", secure: true, httpOnly: true, sameSite: "lax", session: true, storeId: "0" };

describe("toSiteCookie", () => {
  it("maps unknown sameSite values to unspecified and drops a null partition", () => {
    expect(toSiteCookie({ ...wire, sameSite: "weird", partitionKey: null })).toMatchObject({ sameSite: "unspecified" });
    expect(toSiteCookie({ ...wire, partitionKey: null })).not.toHaveProperty("partitionKey");
  });

  it("keeps a partition's top-level site and Firefox's first-party domain", () => {
    expect(toSiteCookie({ ...wire, partitionKey: { topLevelSite: "https://github.com" }, firstPartyDomain: "github.com" })).toMatchObject({
      partitionKey: { topLevelSite: "https://github.com", hasCrossSiteAncestor: false },
      firstPartyDomain: "github.com",
    });
  });
});

describe("webextCookieSource", () => {
  it("merges the browser's queries and drops duplicates", async () => {
    const api = fakeBrowser({ cookies: [wire] });
    const scope = scopeOf("https://github.com/");
    if (!scope.ok) throw new Error();
    const cookies = await webextCookieSource(api, (s) => [{ domain: s.site }, { domain: s.site, partitionKey: {} }]).readSiteCookies(scope.value);
    expect(cookies).toHaveLength(1);
    expect(api.calls.getAll).toHaveLength(2);
  });
});

describe("webextStorageSource", () => {
  it("keeps only string pairs from the page", async () => {
    const api = fakeBrowser();
    api.scripting.executeScript = async () => [{ result: { a: "1", b: 2 } }];
    expect(await webextStorageSource(api).readLocalStorage(7)).toEqual({ a: "1" });
  });

  it("rejects when the page gave nothing", async () => {
    const api = fakeBrowser();
    api.scripting.executeScript = async () => [];
    await expect(webextStorageSource(api).readLocalStorage(7)).rejects.toThrow("storage_unavailable");
  });
});

describe("webextPopupDeps", () => {
  it("remembers the last session", async () => {
    const deps = webextPopupDeps(fakeBrowser(), "ko-KR");
    expect(await deps.loadSession()).toBeNull();
    await deps.saveSession("work");
    expect(await deps.loadSession()).toBe("work");
  });
});
