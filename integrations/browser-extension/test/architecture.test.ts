// The layer rules of src/ (README "구조", .claude/rules/writing-code.md): dependencies point
// inwards, and wire shapes live only in adapters. scripts/check-layers.sh does the same for Rust.

import { readFileSync, readdirSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SRC = fileURLToPath(new URL("../src", import.meta.url));

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const path = join(dir, e.name);
    if (e.isDirectory()) return sources(path);
    return e.name.endsWith(".ts") ? [path] : [];
  });
}

/** Source text without comments, so prose like "the browser." does not count. */
const code = (path: string) =>
  readFileSync(path, "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:"'`])\/\/.*$/gm, "$1");

const FILES = sources(SRC).map((path) => ({ path: relative(SRC, path).split(sep).join("/"), text: code(path) }));
const layer = (name: string) => FILES.filter((f) => f.path.startsWith(`${name}/`));

const importsOf = (text: string) => [...text.matchAll(/\bfrom\s+"([^"]+)"/g)].map((m) => m[1] ?? "");
/** Whether a relative import from `file` lands in one of `layers`. */
const reaches = (file: string, spec: string, layers: readonly string[]) => {
  if (!spec.startsWith(".")) return false;
  const target = join(file, "..", spec).split(sep).join("/");
  return layers.some((l) => target === l || target.startsWith(`${l}/`));
};

const BROWSER_API = /\b(?:chrome|browser)\.|\bglobalThis\b/;

describe.each([
  ["domain", ["adapters", "app", "ui", "ports"]],
  ["ports", ["adapters", "app", "ui"]],
] as const)("src/%s", (name, forbidden) => {
  it("has files to check", () => {
    expect(layer(name).length).toBeGreaterThan(0);
  });

  it("does not touch a browser API", () => {
    expect(layer(name).filter((f) => BROWSER_API.test(f.text)).map((f) => f.path)).toEqual([]);
  });

  it(`does not import ${forbidden.join(", ")}`, () => {
    const bad = layer(name).flatMap((f) => importsOf(f.text).filter((s) => reaches(f.path, s, forbidden)).map((s) => `${f.path}: ${s}`));
    expect(bad).toEqual([]);
  });
});

describe("src/app", () => {
  it("does not import adapters or ui", () => {
    const bad = layer("app").flatMap((f) => importsOf(f.text).filter((s) => reaches(f.path, s, ["adapters", "ui"])).map((s) => `${f.path}: ${s}`));
    expect(bad).toEqual([]);
  });
});

describe("wire shapes", () => {
  // Native Messaging, runtime messages, CDP and browser objects are outside formats; the inside
  // speaks its own words and adapters translate (writing-code.md "wire 모양").
  const WIRE_DECL = /\b(?:interface|type)\s+(\w*Wire|CdpCookieParam)\b/g;

  it("are declared only under src/adapters/", () => {
    const bad = FILES.filter((f) => !f.path.startsWith("adapters/")).flatMap((f) => [...f.text.matchAll(WIRE_DECL)].map((m) => `${f.path}: ${m[1]}`));
    expect(bad).toEqual([]);
  });

  it("are found by the check", () => {
    expect(FILES.some((f) => f.path.startsWith("adapters/") && WIRE_DECL.test(f.text))).toBe(true);
  });
});
