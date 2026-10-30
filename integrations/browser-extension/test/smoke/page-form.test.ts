// The functions the extension runs inside a page to fill and read a login form, in real Chrome on
// a local http site. Skipped without KEYHARBOR_TEST_CHROME.

import { type Server, createServer } from "node:http";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { fillPageLogin, readPageLogin } from "../../src/adapters/webext/page-form.js";
import { type Browser, CHROME, launchChrome } from "../support/chrome.js";

if (!CHROME) console.info("skipped: set KEYHARBOR_TEST_CHROME to a Chrome executable to run");

const PAGE = `<!doctype html><form><input name="q" placeholder="search"><input type="email" id="user"><input type="hidden" id="h">
<input type="password" id="pw"></form><script>window.events = []; for (const id of ["user", "pw"]) document.getElementById(id).addEventListener("input", () => events.push(id));</script>`;

describe.skipIf(!CHROME)("the page-form functions in real Chrome", () => {
  let browser: Browser;
  let server: Server;
  let origin = "";
  let session = "";
  const run = async (expression: string) => (await browser.cdp("Runtime.evaluate", { expression, returnByValue: true }, session)).result?.result?.value;

  beforeAll(async () => {
    server = createServer((_req, res) => res.end(PAGE));
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    const address = server.address();
    origin = `http://127.0.0.1:${typeof address === "object" && address ? address.port : 0}`;
    browser = await launchChrome();
    const { result: target } = await browser.cdp("Target.createTarget", { url: "about:blank" });
    const { result: attached } = await browser.cdp("Target.attachToTarget", { targetId: target.targetId, flatten: true });
    session = attached.sessionId;
    await browser.cdp("Page.enable", {}, session);
    await browser.cdp("Page.navigate", { url: `${origin}/login` }, session);
    for (let i = 0; i < 50 && (await run("document.readyState")) !== "complete"; i++) await new Promise((r) => setTimeout(r, 100));
  }, 60000);
  afterAll(async () => {
    await browser?.close();
    server?.close();
  });

  it("fills the password and the field before it, as if typed, and reads them back", async () => {
    const filled = await run(`(${fillPageLogin.toString()})(${JSON.stringify(origin)}, "amy@x.com", "pw-123")`);
    expect(filled).toBe("filled");
    expect(await run(`[document.getElementById("user").value, document.getElementById("pw").value, document.querySelector("[name=q]").value]`)).toEqual(["amy@x.com", "pw-123", ""]);
    expect(await run("window.events")).toEqual(["user", "pw"]);
    expect(await run(`(${readPageLogin.toString()})(${JSON.stringify(origin)})`)).toEqual({ username: "amy@x.com", password: "pw-123" });
  });

  it("does nothing on another origin", async () => {
    expect(await run(`(${fillPageLogin.toString()})("https://evil.example", "x", "y")`)).toBe("moved");
    expect(await run(`(${readPageLogin.toString()})("https://evil.example")`)).toBeNull();
    expect(await run(`document.getElementById("pw").value`)).toBe("pw-123");
  });
});
