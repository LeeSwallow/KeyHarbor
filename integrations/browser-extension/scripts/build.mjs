// Builds dist/<target>/ for every target in targets.mjs: bundled background and popup scripts,
// the merged manifest, the popup page, and the design-system CSS and icons it uses.

import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";
import { iconNames, stylesheets } from "./design-assets.mjs";
import { mergeManifest } from "./manifest.mjs";
import { TARGETS } from "./targets.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const designDir = resolve(root, "../../design-system");
const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

async function buildTarget(target, outRoot) {
  const out = join(outRoot, target.name);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  await esbuild.build({
    absWorkingDir: root,
    entryPoints: target.entries,
    outdir: out,
    bundle: true,
    format: "iife",
    target: target.esbuildTarget,
    legalComments: "none",
    logLevel: "warning",
  });
  const { version } = readJson(join(root, "package.json"));
  const manifest = mergeManifest(readJson(join(root, "manifest/base.json")), readJson(join(root, target.overlay)));
  writeFileSync(join(out, "manifest.json"), `${JSON.stringify({ ...manifest, version }, null, 2)}\n`);
  cpSync(join(root, "src/ui/popup/popup.html"), join(out, "popup.html"));
  for (const css of stylesheets(designDir)) cpSync(join(designDir, css), join(out, "design", css));
  for (const name of iconNames(join(root, "src/ui"))) cpSync(join(designDir, "icons", `${name}.svg`), join(out, "design/icons", `${name}.svg`));
  cpSync(join(designDir, "icons/LICENSE"), join(out, "design/icons/LICENSE"));
  // The notification asking before a dragged site goes over needs a raster icon (spec 2026-10-31).
  cpSync(join(root, "../../app/icons/128x128.png"), join(out, "icon-128.png"));
  return out;
}

export async function build(outRoot = join(root, "dist"), targets = TARGETS) {
  const outs = [];
  for (const target of targets) outs.push(await buildTarget(target, outRoot));
  return outs;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const outs = await build();
  for (const out of outs) console.log(`built ${out}`);
}
