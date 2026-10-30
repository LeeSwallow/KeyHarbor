// Real headless Chrome over CDP, for the smoke tests. Runs only when KEYHARBOR_TEST_CHROME names a
// Chrome executable, like the Rust browser tests.

import { type ChildProcess, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const CHROME = process.env.KEYHARBOR_TEST_CHROME;

export type Cdp = (method: string, params?: object, sessionId?: string) => Promise<{ result?: any; error?: { message: string } }>;

export interface Browser {
  cdp: Cdp;
  /** Stops Chrome and, once it has exited, removes its profile. */
  close(): Promise<void>;
}

export async function launchChrome(): Promise<Browser> {
  const profile = mkdtempSync(join(tmpdir(), "kh-chrome-"));
  const chrome: ChildProcess = spawn(
    CHROME as string,
    ["--headless=new", "--disable-gpu", "--no-first-run", `--user-data-dir=${profile}`, "--enable-unsafe-extension-debugging", "--remote-debugging-port=0", "about:blank"],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  const url = await new Promise<string>((resolve, reject) => {
    let buf = "";
    chrome.stderr?.on("data", (d) => {
      buf += d;
      const m = buf.match(/DevTools listening on (ws:\S+)/);
      if (m?.[1]) resolve(m[1]);
    });
    setTimeout(() => reject(new Error("Chrome did not start")), 20000);
  });
  const socket = new WebSocket(url);
  await new Promise((r) => socket.addEventListener("open", r));
  let id = 0;
  const pending = new Map<number, (m: any) => void>();
  socket.addEventListener("message", (e) => {
    const m = JSON.parse(String(e.data));
    pending.get(m.id)?.(m);
    pending.delete(m.id);
  });
  const cdp: Cdp = (method, params = {}, sessionId) =>
    new Promise((resolve) => {
      const i = ++id;
      pending.set(i, resolve);
      socket.send(JSON.stringify({ id: i, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  return {
    cdp,
    async close() {
      socket.close();
      const exited = new Promise((r) => chrome.once("exit", r));
      chrome.kill();
      await exited;
      rmSync(profile, { recursive: true, force: true });
    },
  };
}
