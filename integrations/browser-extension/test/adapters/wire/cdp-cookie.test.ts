import { describe, expect, it } from "vitest";
import { toCookieParam } from "../../../src/adapters/wire/cdp-cookie.js";
import { SECRET, cookie } from "../../support/fakes.js";

describe("toCookieParam", () => {
  it("keeps a domain cookie's domain and maps the flags", () => {
    expect(toCookieParam(cookie())).toEqual({
      name: "user_session",
      value: SECRET,
      url: "https://github.com/",
      domain: ".github.com",
      path: "/",
      secure: true,
      httpOnly: true,
      sameSite: "Lax",
      expires: 1893456000,
    });
  });

  it("gives a host-only cookie a url and no domain, so it stays host-only", () => {
    const p = toCookieParam(cookie({ domain: "gist.github.com", hostOnly: true, path: "/a", secure: false }));
    expect(p).toMatchObject({ url: "http://gist.github.com/a", path: "/a" });
    expect(p).not.toHaveProperty("domain");
  });

  it("adds the leading dot to a domain cookie written without one", () => {
    expect(toCookieParam(cookie({ domain: "github.com", hostOnly: false }))).toMatchObject({ domain: ".github.com" });
  });

  it.each([
    ["strict", true, "Strict"],
    ["lax", true, "Lax"],
    ["no_restriction", true, "None"],
    ["no_restriction", false, undefined], // Chrome rejects SameSite=None without Secure
    ["unspecified", true, undefined],
  ] as const)("sameSite %s (secure %s) → %s", (sameSite, secure, expected) => {
    expect(toCookieParam(cookie({ sameSite, secure })).sameSite).toBe(expected);
  });

  it("leaves expires out of session cookies", () => {
    expect(toCookieParam(cookie({ session: true }))).not.toHaveProperty("expires");
  });

  it("carries a Firefox partition of this site as a CDP partition key", () => {
    const p = toCookieParam(cookie({ partitionKey: { topLevelSite: "https://github.com" } }));
    expect(p).toMatchObject({ partitionKey: { topLevelSite: "https://github.com", hasCrossSiteAncestor: false } });
  });
});
