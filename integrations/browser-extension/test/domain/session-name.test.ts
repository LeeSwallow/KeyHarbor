// The rule shared with Rust `SessionName::parse`: rebuild/crates/domain/tests/session_name_contract.rs
// reads the same fixture.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseSessionName } from "../../src/domain/session-name.js";

interface Contract {
  valid: string[];
  invalid: string[];
  trims: Array<[string, string]>;
}
const contract = JSON.parse(readFileSync(new URL("../../../../contracts/session-names.json", import.meta.url), "utf8")) as Contract;

describe("parseSessionName (contracts/session-names.json)", () => {
  it.each(contract.valid)("accepts %j unchanged", (name) => {
    expect(parseSessionName(name)).toEqual({ ok: true, value: name });
  });

  it.each(contract.invalid)("rejects %j", (name) => {
    expect(parseSessionName(name)).toEqual({ ok: false, error: "bad_session" });
  });

  it.each(contract.trims)("trims %j to %j", (input, trimmed) => {
    expect(parseSessionName(input)).toEqual({ ok: true, value: trimmed });
  });
});

describe("parseSessionName beyond the fixture", () => {
  // JSON cannot carry a lone surrogate to Rust, so this case is the extension's alone.
  it("rejects a lone surrogate", () => {
    expect(parseSessionName("a\ud800b")).toEqual({ ok: false, error: "bad_session" });
  });
});
