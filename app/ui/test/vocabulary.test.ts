// The window offers every policy resource and has a word for every usage record kind that Rust
// knows. `vocabulary.json` is checked against `Resource::ALL` and `AuditKind::ALL` by
// app/tests/vocabulary.rs, so a name added in Rust breaks one side or the other (J-175).

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ANY } from "../src/api.js";
import { RESOURCES } from "../src/views/admin.js";
import { SHOWN } from "../src/views/settings.js";
import { auditWords, kindLook, kindWord, resourceWords, valueWarningWords } from "../src/words.js";

const vocabulary = JSON.parse(readFileSync(new URL("./vocabulary.json", import.meta.url), "utf8")) as {
  any: string;
  resources: string[];
  audit_kinds: string[];
  value_warnings: string[];
  settings: string[];
  kinds: string[];
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

  it("has a word and a look for every item kind (R-48 added credential)", () => {
    expect(vocabulary.kinds.filter((k) => kindWord(k) === k)).toEqual([]);
    expect(vocabulary.kinds.filter((k) => !(k in kindLook))).toEqual([]);
  });

  it("names every setting in words, none left as a plain field (J-274)", () => {
    expect(vocabulary.settings.filter((k) => !(k in SHOWN))).toEqual([]);
    expect(Object.keys(SHOWN).filter((k) => !vocabulary.settings.includes(k))).toEqual([]);
  });
});
