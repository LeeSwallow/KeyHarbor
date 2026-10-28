import { describe, expect, it } from "vitest";
import { mergeManifest } from "../../scripts/manifest.mjs";

describe("mergeManifest", () => {
  it("merges objects, joins arrays and lets the overlay win", () => {
    const base = { name: "k", permissions: ["cookies"], action: { default_popup: "p.html" }, v: 1 };
    const overlay = { permissions: ["cookies", "tabs"], action: { default_title: "K" }, v: 2 };
    expect(mergeManifest(base, overlay)).toEqual({ name: "k", permissions: ["cookies", "tabs"], action: { default_popup: "p.html", default_title: "K" }, v: 2 });
    expect(base.permissions).toEqual(["cookies"]); // pure
  });
});
