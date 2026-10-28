import { describe, expect, it } from "vitest";
import { parseCommand, toHandOffCommandWire } from "../../../src/adapters/wire/command.js";
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

describe("the background handler", () => {
  it("runs the use case for a valid command and nothing for an invalid one", async () => {
    const ports = fakePorts();
    const handle = answerCommand(makeCommandHandler(ports));
    expect((await handle(wire)).ok).toBe(true);
    expect((await handle({ type: "nope" })).ok).toBe(false);
    expect(ports.sent).toHaveLength(1);
  });
});
