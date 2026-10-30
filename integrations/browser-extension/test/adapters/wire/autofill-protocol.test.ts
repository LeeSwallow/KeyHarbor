import { describe, expect, it } from "vitest";
import { parseFillReply, parseLoginsReply, parseSaveReply, toAutofillFillWire, toSaveLoginWire } from "../../../src/adapters/wire/protocol.js";

describe("autofill messages", () => {
  it("are built in the host's shape", () => {
    expect(toAutofillFillWire("https://github.com", "github")).toEqual({ v: 1, type: "autofill_fill", origin: "https://github.com", item: "github" });
    expect(toSaveLoginWire("https://github.com", { password: "pw" })).toEqual({ v: 1, type: "save_login", origin: "https://github.com", password: "pw" });
    expect(toSaveLoginWire("https://github.com", { username: "amy", password: "pw" }).username).toBe("amy");
  });

  it("read the host's replies, and anything else is bad_reply", () => {
    expect(parseLoginsReply({ v: 1, type: "logins", items: ["github"] })).toEqual({ ok: true, value: [{ item: "github" }] });
    expect(parseLoginsReply({ v: 1, type: "logins", items: [1] })).toEqual({ ok: false, error: "bad_reply" });
    expect(parseFillReply({ v: 1, type: "filled", username: "amy", password: "pw" })).toEqual({ ok: true, value: { username: "amy", password: "pw" } });
    expect(parseFillReply({ v: 1, type: "error", code: "not_confirmed" })).toEqual({ ok: false, error: "not_confirmed" });
    expect(parseFillReply({ v: 1, type: "error", code: "not_browser" })).toEqual({ ok: false, error: "host_refused" });
    expect(parseFillReply({ v: 2, type: "filled", password: "pw" })).toEqual({ ok: false, error: "bad_reply" });
    expect(parseSaveReply({ v: 1, type: "saved", item: "github~draft-1" })).toEqual({ ok: true, value: { kind: "saved", item: "github~draft-1" } });
    expect(parseSaveReply({ v: 1, type: "known", item: "github" })).toEqual({ ok: true, value: { kind: "known", item: "github" } });
  });
});
