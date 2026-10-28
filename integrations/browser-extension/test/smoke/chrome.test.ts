// Loads the built Chrome extension into real headless Chrome and opens its popup.
// Runs only when KEYHARBOR_TEST_CHROME names a Chrome executable, like the Rust browser tests.
// Branded Chrome ignores --load-extension, so the extension goes in through CDP Extensions.loadUnpacked.

import { type ChildProcess, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { build } from "../../scripts/build.mjs";

const CHROME = process.env.KEYHARBOR_TEST_CHROME;
if (!CHROME) console.info("skipped: set KEYHARBOR_TEST_CHROME to a Chrome executable to run");

type Cdp = (method: string, params?: object, sessionId?: string) => Promise<{ result?: any; error?: { message: string } }>;

let chrome: ChildProcess | undefined;
let ws: WebSocket | undefined;
let cdp: Cdp;
const dirs: string[] = [];

async function launch(): Promise<Cdp> {
  const profile = mkdtempSync(join(tmpdir(), "kh-chrome-"));
  dirs.push(profile);
  chrome = spawn(CHROME as string, ["--headless=new", "--disable-gpu", "--no-first-run", `--user-data-dir=${profile}`, "--enable-unsafe-extension-debugging", "--remote-debugging-port=0", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
  const url = await new Promise<string>((resolve, reject) => {
    let buf = "";
    chrome?.stderr?.on("data", (d) => {
      buf += d;
      const m = buf.match(/DevTools listening on (ws:\S+)/);
      if (m?.[1]) resolve(m[1]);
    });
    setTimeout(() => reject(new Error("Chrome did not start")), 20000);
  });
  const socket = new WebSocket(url);
  ws = socket;
  await new Promise((r) => socket.addEventListener("open", r));
  let id = 0;
  const pending = new Map<number, (m: any) => void>();
  socket.addEventListener("message", (e) => {
    const m = JSON.parse(String(e.data));
    pending.get(m.id)?.(m);
    pending.delete(m.id);
  });
  return (method, params = {}, sessionId) =>
    new Promise((resolve) => {
      const i = ++id;
      pending.set(i, resolve);
      socket.send(JSON.stringify({ id: i, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
}

async function openPopup(extId: string, preload?: string): Promise<string> {
  const { result: target } = await cdp("Target.createTarget", { url: "about:blank" });
  const { result: session } = await cdp("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  await cdp("Page.enable", {}, session.sessionId);
  if (preload) await cdp("Page.addScriptToEvaluateOnNewDocument", { source: preload }, session.sessionId);
  await cdp("Page.navigate", { url: `chrome-extension://${extId}/popup.html` }, session.sessionId);
  for (let i = 0; i < 40; i++) {
    const { result } = await cdp("Runtime.evaluate", { expression: "document.querySelector('#app > *')?.className ?? ''" }, session.sessionId);
    if (result?.result?.value) {
      const { result: html } = await cdp("Runtime.evaluate", { expression: "document.getElementById('app').innerHTML" }, session.sessionId);
      return String(html.result.value);
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("popup did not render");
}

describe.skipIf(!CHROME)("the built extension in real Chrome", () => {
  let extId = "";
  beforeAll(async () => {
    const out = mkdtempSync(join(tmpdir(), "kh-ext-"));
    dirs.push(out);
    await build(out);
    cdp = await launch();
    const loaded = await cdp("Extensions.loadUnpacked", { path: join(out, "chrome") });
    expect(loaded.error).toBeUndefined();
    extId = loaded.result.id;
  }, 60000);
  afterAll(() => {
    ws?.close();
    chrome?.kill();
    for (const d of dirs) rmSync(d, { recursive: true, force: true });
  });

  it("loads with the ID the native host template allows", () => {
    expect(extId).toBe("fimjcgdlhnkdmjnhcccnkpcnmcecgjoc");
  });

  it("refuses its own page as a site to hand off", async () => {
    expect(await openPopup(extId)).toContain("kh-empty");
  });

  it("offers the hand-off for an https tab, drawn with design-system classes", async () => {
    const html = await openPopup(extId, 'chrome.tabs.query = async () => [{ id: 1, url: "https://github.com/settings" }];');
    expect(html).toContain("https://github.com");
    for (const cls of ["kh-dialog", "kh-origins", "kh-btn primary"]) expect(html).toContain(cls);
  });
});
