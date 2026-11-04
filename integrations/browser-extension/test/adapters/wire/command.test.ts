import { describe, expect, it } from "vitest";
import { parseCommand, parseCommandAnswer, toFillLoginWire, toHandOffCommandWire, toListLoginsWire, toSaveLoginWire } from "../../../src/adapters/wire/command.js";
import { answerCommand } from "../../../src/adapters/webext/background.js";
import { makeCommandHandler } from "../../../src/app/background.js";
import { fakePorts } from "../../support/fakes.js";

const wire = { type: "hand_off", tabId: 7, session: "work", ttlSeconds: 3600, includeLocalStorage: false };

describe("parseCommand", () => {
  it("reads a hand_off command", () => {
    expect(parseCommand(wire)).toEqual({ ok: true, value: { type: "hand_off", tabId: 7, request: { session: "work", ttlSeconds: 3600, includeLocalStorage: false } } });
  });

  it("reads what the popup builds", () => {
    expect(toHandOffCommandWire({ tabId: 7, session: "work", ttlSeconds: 3600, includeLocalStorage: false })).toEqual(wire);
  });

  it.each([null, 1, {}, { ...wire, type: "show_cookies" }, { ...wire, tabId: "7" }])("rejects %j", (raw) => {
    expect(parseCommand(raw)).toEqual({ ok: false, error: "bad_command" });
  });

  it("checks the session and TTL with the domain rules", () => {
    expect(parseCommand({ ...wire, session: "bad\nname" })).toEqual({ ok: false, error: "bad_session" });
    expect(parseCommand({ ...wire, ttlSeconds: 5 })).toEqual({ ok: false, error: "bad_ttl" });
  });
});

describe("parseCommand types", () => {
  it.each([
    { ...wire, session: 5 },
    { ...wire, session: undefined },
    { ...wire, ttlSeconds: "3600" },
    { ...wire, includeLocalStorage: "yes" },
    { type: "fill_login", tabId: 7, item: 5 },
  ])("rejects a field of the wrong type in %j", (raw) => {
    expect(parseCommand(raw)).toEqual({ ok: false, error: "bad_command" });
  });

  it("reads what the popup builds for logins", () => {
    expect(parseCommand(toListLoginsWire(7))).toEqual({ ok: true, value: { type: "list_logins", tabId: 7 } });
    expect(parseCommand(toFillLoginWire(7, "github"))).toEqual({ ok: true, value: { type: "fill_login", tabId: 7, item: "github" } });
    expect(parseCommand(toSaveLoginWire(7))).toEqual({ ok: true, value: { type: "save_login", tabId: 7 } });
  });

  it("does not fill a draft or a name the vault would not allow", () => {
    expect(parseCommand(toFillLoginWire(7, "github~draft-1"))).toEqual({ ok: false, error: "bad_command" });
  });
});

describe("parseCommandAnswer", () => {
  const handed = { session: "work", origin: "https://github.com", cookies: 2, localStorage: false };

  it("reads each command's answer", () => {
    expect(parseCommandAnswer("hand_off", { ok: true, value: handed })).toEqual({ ok: true, value: handed });
    expect(parseCommandAnswer("list_logins", { ok: true, value: [{ item: "github" }] })).toEqual({ ok: true, value: [{ item: "github" }] });
    expect(parseCommandAnswer("fill_login", { ok: true, value: { item: "github" } })).toEqual({ ok: true, value: { item: "github" } });
    expect(parseCommandAnswer("save_login", { ok: true, value: { kind: "known", item: "github" } })).toEqual({ ok: true, value: { kind: "known", item: "github" } });
  });

  it("passes on a known error code", () => {
    expect(parseCommandAnswer("hand_off", { ok: false, error: "host_refused" })).toEqual({ ok: false, error: "host_refused" });
  });

  it.each([
    ["hand_off", undefined],
    ["hand_off", "ok"],
    ["hand_off", { ok: true }],
    ["hand_off", { ok: true, value: { ...handed, cookies: "2" } }],
    ["hand_off", { ok: false, error: "made_up" }],
    ["list_logins", { ok: true, value: [{ item: 1 }] }],
    ["list_logins", { ok: true, value: { item: "github" } }],
    ["fill_login", { ok: true, value: {} }],
    ["save_login", { ok: true, value: { kind: "lost", item: "github" } }],
    // A message echoed back is not an answer.
    ["list_logins", { type: "list_logins", tabId: 7 }],
  ] as const)("turns a %s answer shaped %j into bad_reply", (type, raw) => {
    expect(parseCommandAnswer(type, raw)).toEqual({ ok: false, error: "bad_reply" });
  });
});

describe("the background handler", () => {
  it("runs the use case for a valid command and nothing for an invalid one", async () => {
    const ports = fakePorts();
    const handle = answerCommand(makeCommandHandler(ports));
    expect((await handle(wire)).ok).toBe(true);
    expect((await handle({ type: "nope" })).ok).toBe(false);
    expect(ports.sent).toHaveLength(1);
  });
});
