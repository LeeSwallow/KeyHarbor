import { describe, expect, it } from "vitest";
import { startBridge } from "../../src/adapters/webext/bridge.js";
import { fakeBrowser } from "../support/fake-browser.js";

function fakePort() {
  const posted: unknown[] = [];
  let onMessage: (m: unknown) => void = () => undefined;
  let onDisconnect: () => void = () => undefined;
  return {
    posted,
    send: (m: unknown) => onMessage(m),
    drop: () => onDisconnect(),
    port: {
      postMessage: (m: unknown) => void posted.push(m),
      onMessage: { addListener: (l: (m: unknown) => void) => void (onMessage = l) },
      onDisconnect: { addListener: (l: () => void) => void (onDisconnect = l) },
    },
  };
}

describe("the bridge", () => {
  it("opens with bridge and answers each request_site with its id and outcome", async () => {
    const api = fakeBrowser();
    const fake = fakePort();
    api.runtime.connectNative = () => fake.port;
    const asked: Array<[string, string]> = [];
    startBridge(api, async (origin, session) => {
      asked.push([origin, session]);
      return { outcome: "handed_off", cookies: 2 };
    });
    expect(fake.posted).toEqual([{ v: 1, type: "bridge" }]);
    fake.send({ v: 1, type: "request_site", id: 4, session: "work", origin: "https://github.com" });
    fake.send({ v: 1, type: "something_else" });
    await new Promise((r) => setTimeout(r, 0));
    expect(asked).toEqual([["https://github.com", "work"]]);
    expect(fake.posted[1]).toEqual({ v: 1, type: "request_answer", id: 4, outcome: "handed_off", cookies: 2 });
  });

  it("connects again after the host goes away", async () => {
    const api = fakeBrowser();
    let connects = 0;
    const fake = fakePort();
    api.runtime.connectNative = () => {
      connects++;
      return fake.port;
    };
    startBridge(api, async () => ({ outcome: "failed" }), 5);
    fake.drop();
    await new Promise((r) => setTimeout(r, 20));
    expect(connects).toBe(2);
  });
});
