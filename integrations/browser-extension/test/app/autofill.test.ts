import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeFillLogin, makeListLogins, makeSaveLogin } from "../../src/app/autofill.js";
import { parseCommand } from "../../src/adapters/wire/command.js";
import { PASSWORD, fakePorts } from "../support/fakes.js";

const spies: Array<ReturnType<typeof vi.spyOn>> = [];
beforeEach(() => {
  for (const m of ["log", "info", "warn", "error", "debug"] as const) spies.push(vi.spyOn(console, m).mockImplementation(() => {}));
});
afterEach(() => {
  for (const s of spies) expect(s).not.toHaveBeenCalled(); // never logs
  spies.length = 0;
  vi.restoreAllMocks();
});

describe("filling a login", () => {
  it("asks the host for the tab's own origin and puts the values only into the page", async () => {
    const ports = fakePorts();
    const result = await makeFillLogin(ports)(7, "github");
    expect(result).toEqual({ ok: true, value: { item: "github" } });
    expect(ports.sent).toEqual([{ v: 1, type: "autofill_fill", origin: "https://github.com", item: "github" }]);
    expect(ports.filled).toEqual([{ username: "amy", password: PASSWORD }]);
    expect(JSON.stringify(result)).not.toContain(PASSWORD);
  });

  it("puts nothing in when the person did not confirm, and says so", async () => {
    const ports = fakePorts({ reply: { v: 1, type: "error", code: "not_confirmed" } });
    expect(await makeFillLogin(ports)(7, "github")).toEqual({ ok: false, error: "not_confirmed" });
    expect(ports.filled).toEqual([]);
  });

  it("puts nothing in when the page left the site during Touch ID", async () => {
    const ports = fakePorts({ page: { fill: "moved" } });
    expect(await makeFillLogin(ports)(7, "github")).toEqual({ ok: false, error: "page_changed" });
    expect(await makeFillLogin(fakePorts({ page: { fill: "no_form" } }))(7, "github")).toEqual({ ok: false, error: "no_login_form" });
  });

  it("refuses pages that are not sites, and a missing host", async () => {
    expect(await makeFillLogin(fakePorts({ tab: { id: 7, url: "chrome://settings" } }))(7, "github")).toEqual({ ok: false, error: "unsupported_page" });
    expect(await makeFillLogin(fakePorts({ reply: "throws" }))(7, "github")).toEqual({ ok: false, error: "host_unavailable" });
  });

  it("lists names only", async () => {
    const ports = fakePorts();
    expect(await makeListLogins(ports)(7)).toEqual({ ok: true, value: [{ item: "github" }] });
    expect(ports.sent[0]).toEqual({ v: 1, type: "autofill_list", origin: "https://github.com" });
  });
});

describe("saving a login", () => {
  it("sends what the page holds for the tab's origin and answers with the draft's name", async () => {
    const ports = fakePorts();
    const result = await makeSaveLogin(ports)(7);
    expect(result).toEqual({ ok: true, value: { kind: "saved", item: "github~draft-1" } });
    expect(ports.sent[0]).toEqual({ v: 1, type: "save_login", origin: "https://github.com", username: "amy", password: PASSWORD });
    expect(JSON.stringify(result)).not.toContain(PASSWORD);
  });

  it("says when the page has no filled password field", async () => {
    const ports = fakePorts({ page: { read: null } });
    expect(await makeSaveLogin(ports)(7)).toEqual({ ok: false, error: "no_login_form" });
    expect(ports.sent).toEqual([]);
  });
});

describe("popup commands for logins", () => {
  it("name a tab and, for a fill, a valid item; nothing else", () => {
    expect(parseCommand({ type: "fill_login", tabId: 7, item: "work/github" })).toEqual({ ok: true, value: { type: "fill_login", tabId: 7, item: "work/github" } });
    expect(parseCommand({ type: "fill_login", tabId: 7, item: "github~draft-1" })).toEqual({ ok: false, error: "bad_command" });
    expect(parseCommand({ type: "save_login", tabId: 7, password: "x" })).toEqual({ ok: true, value: { type: "save_login", tabId: 7 } });
    expect(parseCommand({ type: "list_logins", tabId: "7" })).toEqual({ ok: false, error: "bad_command" });
  });
});
