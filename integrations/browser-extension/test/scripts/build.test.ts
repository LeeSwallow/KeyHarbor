// Builds both targets into a temporary folder and checks what a browser will load.

import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { build } from "../../scripts/build.mjs";
import { iconNames } from "../../scripts/design-assets.mjs";
import { HOST_NAME } from "../../src/adapters/wire/protocol.js";

let out: string;
beforeAll(async () => {
  out = mkdtempSync(join(tmpdir(), "kh-ext-"));
  await build(out);
});
afterAll(() => rmSync(out, { recursive: true, force: true }));

const read = (...p: string[]) => readFileSync(join(out, ...p), "utf8");

describe.each(["chrome", "firefox"])("dist/%s", (target) => {
  it("has the manifest, scripts and popup page", () => {
    const manifest = JSON.parse(read(target, "manifest.json"));
    expect(manifest.manifest_version).toBe(3);
    expect(manifest.permissions).toEqual(["cookies", "nativeMessaging", "activeTab", "scripting", "storage"]);
    expect(manifest.host_permissions).toBeUndefined(); // asked for at use time
    for (const f of ["background.js", "popup.js", "popup.html"]) expect(existsSync(join(out, target, f))).toBe(true);
  });

  it("ships the design-system layers the popup imports", () => {
    expect(read(target, "popup.html")).toContain('href="design/keyharbor.css"');
    const entry = read(target, "design/keyharbor.css");
    for (const m of entry.matchAll(/@import url\("([^"]+)"\)/g)) expect(existsSync(join(out, target, "design", m[1] ?? ""))).toBe(true);
    expect(existsSync(join(out, target, "design/patterns/extension-popup.css"))).toBe(true);
  });

  it("ships every icon the popup draws, with the licence", () => {
    const icons = iconNames(join(__dirname, "../../src/ui"));
    expect(icons).toContain("hand");
    for (const name of icons) expect(existsSync(join(out, target, "design/icons", `${name}.svg`))).toBe(true);
    expect(existsSync(join(out, target, "design/icons/LICENSE"))).toBe(true);
  });
});

describe("browser differences", () => {
  it("chrome runs a service worker, firefox background scripts with a gecko id", () => {
    const chrome = JSON.parse(read("chrome", "manifest.json"));
    const firefox = JSON.parse(read("firefox", "manifest.json"));
    expect(chrome.background).toEqual({ service_worker: "background.js" });
    expect(firefox.background).toEqual({ scripts: ["background.js"] });
    expect(firefox.browser_specific_settings.gecko.id).toBe("browser-extension@keyharbor.dev");
    expect(chrome.browser_specific_settings).toBeUndefined();
  });

  it("the bundles read their own namespace", () => {
    expect(read("chrome", "background.js")).toContain("globalThis.chrome");
    expect(read("firefox", "background.js")).toContain("globalThis.browser");
  });
});

describe("native host templates", () => {
  const template = (browser: string) => JSON.parse(readFileSync(join(__dirname, `../../native-host/${browser}.dev.keyharbor.host.json`), "utf8"));

  it("allow exactly this extension in each browser", () => {
    // Chrome derives an unpacked extension's ID from the manifest key: sha256 of the key, first 32 hex digits as a–p.
    const key = Buffer.from(JSON.parse(read("chrome", "manifest.json")).key, "base64");
    const id = [...createHash("sha256").update(key).digest("hex").slice(0, 32)].map((h) => String.fromCharCode(97 + Number.parseInt(h, 16))).join("");
    expect(template("chrome").allowed_origins).toEqual([`chrome-extension://${id}/`]);
    const gecko = JSON.parse(read("firefox", "manifest.json")).browser_specific_settings.gecko.id;
    expect(template("firefox").allowed_extensions).toEqual([gecko]);
  });

  it("name the host the extension calls", () => {
    for (const b of ["chrome", "firefox"]) expect(template(b)).toMatchObject({ name: HOST_NAME, type: "stdio" });
  });
});
