import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeHandOffSite } from "../../src/app/hand-off-site.js";
import { SECRET, cookie, fakePorts } from "../support/fakes.js";

const request = { session: "work", ttlSeconds: 3600, includeLocalStorage: false };
const consoleSpies: Array<ReturnType<typeof vi.spyOn>> = [];

beforeEach(() => {
  for (const m of ["log", "info", "warn", "error", "debug"] as const) consoleSpies.push(vi.spyOn(console, m).mockImplementation(() => {}));
});
afterEach(() => {
  for (const s of consoleSpies) expect(s).not.toHaveBeenCalled(); // the use case never logs
  consoleSpies.length = 0;
  vi.restoreAllMocks();
});

describe("handOffSite", () => {
  it("sends exactly one receive_site message with the site's cookies", async () => {
    const ports = fakePorts({ cookies: [cookie(), cookie({ name: "other", domain: ".evil.io" })] });
    const result = await makeHandOffSite(ports)(7, request);
    expect(result).toEqual({ ok: true, value: { session: "work", origin: "https://github.com", cookies: 1, localStorage: false } });
    expect(ports.sent).toHaveLength(1);
    expect(ports.sent[0]).toMatchObject({ v: 1, type: "receive_site", origin: "https://github.com", session: "work", ttl_seconds: 3600 });
    expect(ports.sent[0]?.cookies.map((c) => c.name)).toEqual(["user_session"]);
    expect(ports.sent[0]).not.toHaveProperty("local_storage");
  });

  it("adds localStorage only when asked", async () => {
    const ports = fakePorts();
    const result = await makeHandOffSite(ports)(7, { ...request, includeLocalStorage: true });
    expect(result.ok && result.value.localStorage).toBe(true);
    expect(ports.sent[0]?.local_storage).toEqual({ token: SECRET });
  });

  it.each([
    ["the tab is gone", { tab: null }, "unsupported_page"],
    ["the page is not http(s)", { tab: { id: 7, url: "chrome://settings" } }, "unsupported_page"],
    ["cookies cannot be read", { cookies: "throws" as const }, "no_permission"],
    ["there are no cookies of the site", { cookies: [cookie({ domain: ".evil.io" })] }, "no_cookies"],
    ["the host is not installed", { reply: "throws" as const }, "host_unavailable"],
    ["the host refuses", { reply: { v: 1, type: "error", code: "no_session" } }, "host_refused"],
    ["the host answers garbage", { reply: { v: 9 } }, "bad_reply"],
  ])("returns a code, not a value, when %s", async (_why, opts, code) => {
    const result = await makeHandOffSite(fakePorts(opts))(7, request);
    expect(result).toEqual({ ok: false, error: code });
    expect(JSON.stringify(result)).not.toContain(SECRET);
  });

  it("sends nothing when it cannot read the page's localStorage", async () => {
    const ports = fakePorts({ storage: "throws" });
    expect(await makeHandOffSite(ports)(7, { ...request, includeLocalStorage: true })).toEqual({ ok: false, error: "storage_unavailable" });
    expect(ports.sent).toHaveLength(0);
  });

  it("sends nothing before checking the page", async () => {
    const ports = fakePorts({ tab: { id: 7, url: "file:///Users/me/x.html" } });
    await makeHandOffSite(ports)(7, request);
    expect(ports.sent).toHaveLength(0);
  });
});
