// Builds dist/: the bundled window script, the page, and the design-system stylesheets it
// imports (copied at build time, never into src/).

import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
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
// Icons, and a class per icon: the CSP has no 'unsafe-inline', so the design system's inline
// `style="mask-image:…"` cannot be used; `<span class="kh-i i-key">` takes it from here.
cpSync(join(designDir, "icons"), join(out, "design", "icons"), { recursive: true });
// The app icon (docs/design/assets, also the bundle's icon) at the top of the side bar.
cpSync(resolve(root, "../../docs/design/assets/app-icon.svg"), join(out, "app-icon.svg"));
const icons = readdirSync(join(designDir, "icons")).filter((f) => f.endsWith(".svg")).map((f) => f.slice(0, -4));
writeFileSync(join(out, "icons.css"), icons.map((i) => `.kh-i.i-${i} { mask-image: url(design/icons/${i}.svg); }`).join("\n") + "\n");
console.log(`built ${out}`);
