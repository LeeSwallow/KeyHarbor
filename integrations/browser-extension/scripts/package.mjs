// Store packages (#124): builds every target, then writes to web-ext-artifacts/
//   keyharbor-chrome-<v>.zip   dist/chrome without the manifest `key` (the Chrome Web Store
//                              assigns its own; a `key` field is refused on upload)
//   keyharbor-firefox-<v>.zip  dist/firefox, for addons.mozilla.org
//   keyharbor-extension-source-<v>.zip  the sources AMO asks for (the scripts are bundled)

import { cpSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import webExt from "web-ext";
import { build } from "./build.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const artifacts = join(root, "web-ext-artifacts");

/// The manifest a store gets: the same, without the unpacked build's fixed `key`.
export function storeManifest(manifest) {
  const { key: _key, ...rest } = manifest;
  return rest;
}

async function zip(sourceDir, filename) {
  await webExt.cmd.build({ sourceDir, artifactsDir: artifacts, filename, overwriteDest: true }, { shouldExitProgram: false });
  return join(artifacts, filename);
}

export async function packageAll() {
  const { version } = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  await build();
  const store = join(root, "dist/store-chrome");
  rmSync(store, { recursive: true, force: true });
  cpSync(join(root, "dist/chrome"), store, { recursive: true });
  const manifestPath = join(store, "manifest.json");
  writeFileSync(manifestPath, `${JSON.stringify(storeManifest(JSON.parse(readFileSync(manifestPath, "utf8"))), null, 2)}\n`);
  const out = [
    await zip(store, `keyharbor-chrome-${version}.zip`),
    await zip(join(root, "dist/firefox"), `keyharbor-firefox-${version}.zip`),
  ];
  const source = join(artifacts, `keyharbor-extension-source-${version}.zip`);
  const repo = resolve(root, "../..");
  execFileSync("git", ["-C", repo, "archive", "--format=zip", "-o", source, "HEAD",
    "integrations/browser-extension", "design-system", "app/icons", "LICENSE-MIT", "LICENSE-APACHE"]);
  out.push(source);
  return out;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  for (const file of await packageAll()) console.log(file);
}
