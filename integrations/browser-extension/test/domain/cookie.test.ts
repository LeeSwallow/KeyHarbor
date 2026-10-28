import { describe, expect, it } from "vitest";
import { selectSiteCookies, whyLeftBehind } from "../../src/domain/cookie.js";
import { scopeOf } from "../../src/domain/scope.js";
import { SECRET, cookie } from "../support/fakes.js";

const github = (() => {
  const r = scopeOf("https://github.com/");
  if (!r.ok) throw new Error();
  return r.value;
})();

describe("whyLeftBehind", () => {
  it("lets this site's cookies over", () => {
    expect(whyLeftBehind(cookie(), github)).toBeNull();
    expect(whyLeftBehind(cookie({ domain: "gist.github.com", hostOnly: true }), github)).toBeNull();
  });

  it("refuses cookies of other sites", () => {
    expect(whyLeftBehind(cookie({ domain: ".evilgithub.com" }), github)).toBe("out_of_scope");
  });

  it("lets a partition of this site over", () => {
    expect(whyLeftBehind(cookie({ partitionKey: { topLevelSite: "https://github.com" } }), github)).toBeNull();
  });

  it("leaves cookies partitioned under another top-level site behind", () => {
    expect(whyLeftBehind(cookie({ partitionKey: { topLevelSite: "https://example.com" } }), github)).toBe("other_partition");
  });

  it("keeps only this site's first-party-isolated cookies", () => {
    expect(whyLeftBehind(cookie({ firstPartyDomain: "github.com" }), github)).toBeNull();
    expect(whyLeftBehind(cookie({ firstPartyDomain: "example.com" }), github)).toBe("other_first_party");
  });
});

describe("selectSiteCookies", () => {
  it("counts what stayed behind without naming it", () => {
    const { cookies, skipped } = selectSiteCookies(
      [cookie(), cookie({ domain: ".evil.io" }), cookie({ partitionKey: { topLevelSite: "https://x.com" } })],
      github,
    );
    expect(cookies).toHaveLength(1);
    expect(skipped).toEqual({ out_of_scope: 1, other_partition: 1, other_first_party: 0 });
    expect(JSON.stringify(skipped)).not.toContain(SECRET);
  });
});
