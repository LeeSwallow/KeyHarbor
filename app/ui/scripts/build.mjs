// Builds dist/: the bundled window script, the page, and the design-system stylesheets it
// imports (copied at build time, never into src/).

import { cpSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const designDir = resolve(root, "../../design-system");
const out = join(root, "dist");

/** keyharbor.css and the files it @imports, relative to design-system/. */
function stylesheets() {
  const css = readFileSync(join(designDir, "keyharbor.css"), "utf8");
  const imports = [...css.matchAll(/@import\s+url\(\s*["']?([^"')]+)["']?\s*\)/g)].map((m) => m[1]);
  return ["keyharbor.css", ...imports];
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
await esbuild.build({
  absWorkingDir: root,
  entryPoints: ["src/main.ts"],
  outfile: join(out, "main.js"),
  bundle: true,
  format: "iife",
  target: "safari16",
  legalComments: "none",
  logLevel: "warning",
});
cpSync(join(root, "src/index.html"), join(out, "index.html"));
cpSync(join(root, "src/app.css"), join(out, "app.css"));
for (const css of stylesheets()) cpSync(join(designDir, css), join(out, "design", css));
console.log(`built ${out}`);
