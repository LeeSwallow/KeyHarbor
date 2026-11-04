// The window offers every policy resource and has a word for every usage record kind that Rust
// knows. `vocabulary.json` is checked against `Resource::ALL` and `AuditKind::ALL` by
// app/tests/vocabulary.rs, so a name added in Rust breaks one side or the other (J-175).

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ANY } from "../src/api.js";
import { RESOURCES } from "../src/views/admin.js";
import { auditWords, resourceWords, valueWarningWords } from "../src/words.js";

const vocabulary = JSON.parse(readFileSync(new URL("./vocabulary.json", import.meta.url), "utf8")) as {
  any: string;
  resources: string[];
  audit_kinds: string[];
  value_warnings: string[];
};

describe("the window's vocabulary", () => {
  it("offers a rule for every resource Rust knows", () => {
    expect([...RESOURCES].sort()).toEqual([...vocabulary.resources].sort());
  });

  it("has a word for every resource", () => {
    expect(vocabulary.resources.filter((r) => !(r in resourceWords))).toEqual([]);
  });

  it("means every agent or target with Rust's wildcard", () => {
    expect(ANY).toBe(vocabulary.any);
  });

  it("has a word for every usage record kind", () => {
    expect(vocabulary.audit_kinds.filter((k) => !(k in auditWords))).toEqual([]);
  });

  it("has a sentence for every reason a saved value warns (J-230)", () => {
    expect(vocabulary.value_warnings.filter((r) => !(r in valueWarningWords))).toEqual([]);
  });
});
