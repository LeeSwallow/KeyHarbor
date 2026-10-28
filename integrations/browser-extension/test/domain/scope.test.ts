import { describe, expect, it } from "vitest";
import { hostPatterns, inScope, isSitePartition, scopeOf } from "../../src/domain/scope.js";

const scope = (url: string) => {
  const r = scopeOf(url);
  if (!r.ok) throw new Error(r.error);
  return r.value;
};

describe("scopeOf", () => {
  it("uses the registrable domain of the tab", () => {
    expect(scope("https://gist.github.com/x?y=1")).toEqual({ origin: "https://gist.github.com", host: "gist.github.com", site: "github.com" });
    expect(scope("https://www.bbc.co.uk/").site).toBe("bbc.co.uk");
  });

  it("treats private public suffixes as sites of their own", () => {
    expect(scope("https://foo.github.io/").site).toBe("foo.github.io");
  });

  it("falls back to the host for addresses and single labels", () => {
    expect(scope("http://127.0.0.1:8080/").site).toBe("127.0.0.1");
    expect(scope("http://localhost:3000/").site).toBe("localhost");
  });

  it.each(["file:///etc/hosts", "about:blank", "chrome://settings", "chrome-extension://abc/popup.html", "moz-extension://abc/popup.html", "view-source:https://github.com", "data:text/html,hi", "not a url", ""])(
    "refuses %s",
    (url) => {
      expect(scopeOf(url)).toEqual({ ok: false, error: "unsupported_page" });
    },
  );
});

describe("inScope", () => {
  const s = scope("https://github.com/");
  it.each([
    [".github.com", true],
    ["github.com", true],
    ["gist.github.com", true],
    [".api.github.com", true],
    ["evilgithub.com", false],
    ["github.com.evil.io", false],
    ["githubusercontent.com", false],
  ])("%s → %s", (domain, expected) => {
    expect(inScope(s, domain)).toBe(expected);
  });
});

describe("partitions and host patterns", () => {
  it("matches a partition's top-level site", () => {
    const s = scope("https://github.com/");
    expect(isSitePartition(s, "https://github.com")).toBe(true);
    expect(isSitePartition(s, "https://example.com")).toBe(false);
    expect(isSitePartition(s, "garbage")).toBe(false);
  });

  it("asks for the site and its subdomains, or just the host for addresses", () => {
    expect(hostPatterns(scope("https://gist.github.com/"))).toEqual(["*://github.com/*", "*://*.github.com/*"]);
    expect(hostPatterns(scope("http://127.0.0.1:8080/"))).toEqual(["*://127.0.0.1/*"]);
    expect(hostPatterns(scope("http://localhost/"))).toEqual(["*://localhost/*"]);
  });
});
