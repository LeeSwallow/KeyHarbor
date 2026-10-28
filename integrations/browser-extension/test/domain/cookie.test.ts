import { describe, expect, it } from "vitest";
import { toCookieParam, toCookieParams } from "../../src/domain/cookie.js";
import { scopeOf } from "../../src/domain/scope.js";
import { SECRET, cookie } from "../support/fakes.js";

const github = (() => {
  const r = scopeOf("https://github.com/");
  if (!r.ok) throw new Error();
  return r.value;
})();

describe("toCookieParam", () => {
  it("keeps a domain cookie's domain and maps the flags", () => {
    expect(toCookieParam(cookie(), github)).toEqual({
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
    const p = toCookieParam(cookie({ domain: "gist.github.com", hostOnly: true, path: "/a", secure: false }), github);
    expect(p).toMatchObject({ url: "http://gist.github.com/a", path: "/a" });
    expect(p).not.toHaveProperty("domain");
  });

  it("adds the leading dot to a domain cookie written without one", () => {
    expect(toCookieParam(cookie({ domain: "github.com", hostOnly: false }), github)).toMatchObject({ domain: ".github.com" });
  });

  it.each([
    ["strict", true, "Strict"],
    ["lax", true, "Lax"],
    ["no_restriction", true, "None"],
    ["no_restriction", false, undefined], // Chrome rejects SameSite=None without Secure
    ["unspecified", true, undefined],
  ] as const)("sameSite %s (secure %s) → %s", (sameSite, secure, expected) => {
    const p = toCookieParam(cookie({ sameSite, secure }), github);
    expect(typeof p === "object" ? p.sameSite : p).toBe(expected);
  });

  it("leaves expires out of session cookies", () => {
    const p = toCookieParam(cookie({ session: true }), github);
    expect(p).not.toHaveProperty("expires");
  });

  it("refuses cookies of other sites", () => {
    expect(toCookieParam(cookie({ domain: ".evilgithub.com" }), github)).toBe("out_of_scope");
  });

  it("carries a Firefox partition of this site as a CDP partition key", () => {
    const p = toCookieParam(cookie({ partitionKey: { topLevelSite: "https://github.com" } }), github);
    expect(p).toMatchObject({ partitionKey: { topLevelSite: "https://github.com", hasCrossSiteAncestor: false } });
  });

  it("leaves cookies partitioned under another top-level site behind", () => {
    expect(toCookieParam(cookie({ partitionKey: { topLevelSite: "https://example.com" } }), github)).toBe("other_partition");
  });

  it("keeps only this site's first-party-isolated cookies", () => {
    expect(toCookieParam(cookie({ firstPartyDomain: "github.com" }), github)).not.toBeTypeOf("string");
    expect(toCookieParam(cookie({ firstPartyDomain: "example.com" }), github)).toBe("other_first_party");
  });
});

describe("toCookieParams", () => {
  it("counts what stayed behind without naming it", () => {
    const { params, skipped } = toCookieParams(
      [cookie(), cookie({ domain: ".evil.io" }), cookie({ partitionKey: { topLevelSite: "https://x.com" } })],
      github,
    );
    expect(params).toHaveLength(1);
    expect(skipped).toEqual({ out_of_scope: 1, other_partition: 1, other_first_party: 0 });
    expect(JSON.stringify(skipped)).not.toContain(SECRET);
  });
});
