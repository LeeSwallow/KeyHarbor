// Which files of design-system/ the popup needs: the entry stylesheet and everything it imports, and
// the icons the popup code names with icon("…"). They are copied at build time, never into src/.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

/** keyharbor.css and the files it @imports, relative to design-system/. */
export function stylesheets(designDir) {
  const entry = "keyharbor.css";
  const css = readFileSync(join(designDir, entry), "utf8");
  const imports = [...css.matchAll(/@import\s+url\(\s*["']?([^"')]+)["']?\s*\)/g)].map((m) => m[1]);
  return [entry, ...imports];
}

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

/** Icon names used by icon("name") in the given source folder. */
export function iconNames(srcDir) {
  const names = new Set();
  for (const file of files(srcDir).filter((f) => f.endsWith(".ts"))) {
    for (const m of readFileSync(file, "utf8").matchAll(/icon\("([a-z0-9-]+)"/g)) {
      names.add(m[1]);
    }
  }
  return [...names].sort();
}
