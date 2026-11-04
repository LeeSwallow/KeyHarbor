// The window offers every policy resource and has a word for every usage record kind that Rust
// knows. `vocabulary.json` is checked against `Resource::ALL` and `AuditKind::ALL` by
// app/tests/vocabulary.rs, so a name added in Rust breaks one side or the other (J-175).

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { RESOURCES } from "../src/views/admin.js";
import { auditWords, resourceWords } from "../src/words.js";

const vocabulary = JSON.parse(readFileSync(new URL("./vocabulary.json", import.meta.url), "utf8")) as {
  resources: string[];
  audit_kinds: string[];
};

describe("the window's vocabulary", () => {
  it("offers a rule for every resource Rust knows", () => {
    expect([...RESOURCES].sort()).toEqual([...vocabulary.resources].sort());
  });

  it("has a word for every resource", () => {
    expect(vocabulary.resources.filter((r) => !(r in resourceWords))).toEqual([]);
  });

  it("has a word for every usage record kind", () => {
    expect(vocabulary.audit_kinds.filter((k) => !(k in auditWords))).toEqual([]);
  });
});
