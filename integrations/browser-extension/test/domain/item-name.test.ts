// The rule shared with Rust `ItemName::parse_new`: crates/domain/tests/item_names_contract.rs
// reads the same fixture.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { isFillableItemName } from "../../src/domain/item-name.js";

interface Contract {
  valid: string[];
  invalid: string[];
}
const contract = JSON.parse(readFileSync(new URL("../../../../contracts/item-names.json", import.meta.url), "utf8")) as Contract;

describe("isFillableItemName (contracts/item-names.json)", () => {
  it.each(contract.valid)("accepts %j", (name) => {
    expect(isFillableItemName(name)).toBe(true);
  });

  it.each(contract.invalid)("rejects %j", (name) => {
    expect(isFillableItemName(name)).toBe(false);
  });
});

describe("isFillableItemName beyond the fixture", () => {
  // JSON cannot carry a lone surrogate to Rust, so this case is the extension's alone.
  it("rejects a lone surrogate", () => {
    expect(isFillableItemName("a\ud800b")).toBe(false);
  });
});
