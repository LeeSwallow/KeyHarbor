// Screenshots of the window (09, .claude/rules/visualize.md: light and dark): the built page in
// headless Chrome with a stand-in `__TAURI__` that answers every command with sample data, so
// no vault or app is needed. Values shown are made up.
//
//   KEYHARBOR_TEST_CHROME=<chrome> node scripts/shots.mjs <out-dir> [screen…]
//
// Screens: sessions, settings, identity. Each is written as <screen>.png and <screen>-dark.png.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const chrome = process.env.KEYHARBOR_TEST_CHROME;
if (!chrome) {
  console.error("set KEYHARBOR_TEST_CHROME to a Chrome executable");
  process.exit(2);
}
const [out = "shots", ...wanted] = process.argv.slice(2);
const page = pathToFileURL(resolve(import.meta.dirname, "../dist/index.html")).href;

const session = (patch) => ({
  id: "s1", name: "jobs", state: "running", cdp: false, description: "채용 공고 확인", sites: [],
  browser: "chrome", protection: "owned", failure: null, ...patch,
});
const identityFields = [
  ["full_name", false], ["given_name", false], ["family_name", false], ["email", false], ["phone", false],
  ["birth_date", false], ["english_name", false], ["hanja_name", false], ["gender", false], ["nationality", false],
  ["emergency_phone", false], ["military_service", true], ["disability", true], ["veterans", true], ["note", true],
];
const answers = {
  overview: { vault_ready: true, sessions_open: 2, requests_pending: 0, drafts: 0 },
  inbox: { requests: [], drafts: [] },
  sessions: [
    session({ id: "s1", name: "jobs", browser: "ego-lite", protection: "shared",
      sites: [{ origin: "https://jasoseol.com", state: "logged_in", item: "jasoseol", checked_at: "" }] }),
    session({ id: "s2", name: "work", description: "GitHub 이슈 정리" }),
    session({ id: "s3", name: "apply", state: "stopped", description: null }),
    session({ id: "s4", name: "old", state: "failed", description: null, failure: "ego lite is not available: `ego-browser` did not answer within 5s" }),
  ],
  items: [],
  audit: [],
  handoffs: [],
  settings: [
    ["audit_enabled", "true", "true"], ["capture_enabled", "false", "false"],
    ["browser", "auto", "auto"], ["ego_probe_seconds", "5", "5"], ["ego_poll_ms", "20", "20"],
    ["upload_dirs", "~/Documents/Recruiting:~/Pictures/증명사진", ""], ["upload_max_files", "10", "10"],
    ["default_item_ttl", "none", "none"], ["draft_ttl", "7d", "7d"], ["clipboard_clear_seconds", "30", "30"],
    ["notify_requests", "true", "true"], ["notify_handoffs", "true", "true"], ["notify_drafts", "true", "true"], ["notify_denials", "true", "true"],
  ].map(([key, value, def]) => ({ key, value, default: def })),
  policies: [],
  kinds: [
    { kind: "identity", domain: "personal", fields: identityFields, free_fields: false, custom_fields: true },
    { kind: "login", domain: "credentials", fields: [["username", false], ["password", true]], free_fields: false, custom_fields: false },
  ],
  browsers: { ego_lite: true },
};

/// Clicks that bring each screen up, after the window loaded.
const screens = {
  sessions: `document.querySelector('[data-action="go"][data-view="sessions"]').click()`,
  settings: `document.querySelector('[data-action="go"][data-view="settings"]').click()`,
  identity: `document.querySelector('[data-action="go"][data-view="items"]').click();
    document.querySelector('[data-action="new-item"]').click();
    const kind = document.querySelector('select[name="kind"]'); kind.value = "identity";
    kind.dispatchEvent(new Event("change", { bubbles: true }));
    document.querySelector('[data-action="add-custom-field"]').click();
    document.querySelector('[name="cn:0"]').value = "school";
    document.querySelector('[name="cn:1"]').value = "gpa";`,
};

const profile = mkdtempSync(join(tmpdir(), "kh-shots-"));
const browser = spawn(chrome, [
  "--headless=new", `--user-data-dir=${profile}`, "--remote-debugging-port=0", "--no-first-run",
  "--allow-file-access-from-files", "--window-size=1280,860", "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });
const endpoint = await new Promise((done, fail) => {
  let text = "";
  browser.stderr.on("data", (chunk) => {
    text += chunk;
    const found = text.match(/ws:\/\/[^\s]+/);
    if (found) done(found[0]);
  });
  browser.on("exit", () => fail(new Error(`chrome exited: ${text}`)));
});

const ws = new WebSocket(endpoint);
await new Promise((done) => ws.addEventListener("open", done));
let next = 0;
const waiting = new Map();
ws.addEventListener("message", (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && waiting.has(msg.id)) {
    waiting.get(msg.id)(msg);
    waiting.delete(msg.id);
  }
});
const call = (method, params = {}, sessionId) =>
  new Promise((done, fail) => {
    const id = ++next;
    waiting.set(id, (msg) => (msg.error ? fail(new Error(`${method}: ${msg.error.message}`)) : done(msg.result)));
    ws.send(JSON.stringify({ id, method, params, sessionId }));
  });
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

const { targetId } = await call("Target.createTarget", { url: "about:blank" });
const { sessionId } = await call("Target.attachToTarget", { targetId, flatten: true });
const on = (method, params) => call(method, params, sessionId);
await on("Page.enable");
await on("Emulation.setDeviceMetricsOverride", { width: 1280, height: 860, deviceScaleFactor: 2, mobile: false });
const stand_in = `window.__TAURI__ = { core: { invoke: async (cmd) => structuredClone(${JSON.stringify(answers)}[cmd] ?? null) } };`;
await on("Page.addScriptToEvaluateOnNewDocument", { source: stand_in });

mkdirSync(out, { recursive: true });
for (const name of wanted.length ? wanted : Object.keys(screens)) {
  for (const theme of ["light", "dark"]) {
    await on("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
    await on("Page.navigate", { url: page });
    await sleep(800);
    const ran = await on("Runtime.evaluate", { expression: screens[name], awaitPromise: false });
    if (ran.exceptionDetails) throw new Error(`${name}: ${ran.exceptionDetails.exception?.description}`);
    await sleep(500);
    const shot = await on("Page.captureScreenshot", { format: "png" });
    const file = join(out, theme === "dark" ? `${name}-dark.png` : `${name}.png`);
    writeFileSync(file, Buffer.from(shot.data, "base64"));
    console.log(file);
  }
}
ws.close();
const exited = new Promise((done) => browser.once("exit", done));
browser.kill();
await exited;
rmSync(profile, { recursive: true, force: true, maxRetries: 3 });
