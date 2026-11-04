import { describe, expect, it } from "vitest";
import { err, ok } from "../../src/domain/result.js";
import { makePopupController } from "../../src/ui/popup/controller.js";
import type { PopupDeps } from "../../src/ui/popup/deps.js";
import type { PopupState } from "../../src/ui/popup/view.js";

const outcome = { session: "work", origin: "https://github.com", cookies: 2, localStorage: false };

function deps(over: Partial<PopupDeps> = {}): PopupDeps & { handOffs: number } {
  const d = {
    handOffs: 0,
    language: "en",
    activeTab: async () => ({ id: 7, url: "https://github.com/settings" }),
    requestHostAccess: async () => true,
    handOff: async () => {
      d.handOffs += 1;
      return ok(outcome);
    },
    listLogins: async () => ok([]),
    fillLogin: async (_tabId: number, item: string) => ok({ item }),
    saveLogin: async () => ok({ kind: "saved" as const, item: "github~draft-1" }),
    loadSession: async () => "work",
    saveSession: async () => undefined,
    ...over,
  };
  return d;
}

async function started(d: PopupDeps) {
  const states: PopupState[] = [];
  const popup = makePopupController(d, (s) => states.push(s));
  await popup.start();
  return { popup, last: () => states[states.length - 1] };
}

describe("the popup controller", () => {
  it("lets the button be pressed again after a rejected hand-off", async () => {
    let calls = 0;
    const d = deps({
      handOff: async () => {
        calls += 1;
        if (calls === 1) throw new Error("background gone");
        return ok(outcome);
      },
    });
    const { popup, last } = await started(d);
    await popup.onHandOff();
    expect(last()).toMatchObject({ kind: "ready", busy: false, error: "host_unavailable" });
    await popup.onHandOff();
    expect(calls).toBe(2);
    expect(last()).toMatchObject({ kind: "ready", busy: false, done: { cookies: 2, session: "work" } });
  });

  it("shows a refused hand-off as its error and stays usable", async () => {
    const { popup, last } = await started(deps({ handOff: async () => err("host_refused") }));
    await popup.onHandOff();
    expect(last()).toMatchObject({ busy: false, error: "host_refused" });
  });

  it("still reports a hand-off whose session could not be remembered", async () => {
    const { popup, last } = await started(deps({ saveSession: () => Promise.reject(new Error("storage full")) }));
    await popup.onHandOff();
    expect(last()).toMatchObject({ busy: false, done: { cookies: 2, session: "work" } });
  });

  it("does not start a second hand-off while one runs", async () => {
    const d = deps();
    const { popup } = await started(d);
    await Promise.all([popup.onHandOff(), popup.onHandOff()]);
    expect(d.handOffs).toBe(1);
  });

  it("says the page cannot be handed off when the active tab cannot be read", async () => {
    const { last } = await started(deps({ activeTab: () => Promise.reject(new Error("no tabs")) }));
    expect(last()).toEqual({ kind: "unsupported" });
  });

  it("falls back to the default session when the last one cannot be read", async () => {
    const { last } = await started(deps({ loadSession: () => Promise.reject(new Error("no storage")) }));
    expect(last()).toMatchObject({ kind: "ready", session: "work", busy: false });
  });

  it("turns rejected fills and saves into errors", async () => {
    const d = deps({ fillLogin: () => Promise.reject(new Error("x")), saveLogin: () => Promise.reject(new Error("y")) });
    const { popup, last } = await started(d);
    await popup.onFill("github");
    expect(last()).toMatchObject({ busy: false, error: "host_unavailable" });
    await popup.onSaveLogin();
    expect(last()).toMatchObject({ busy: false, error: "host_unavailable" });
  });

  it("hands nothing off without host access", async () => {
    const d = deps({ requestHostAccess: async () => false });
    const { popup, last } = await started(d);
    await popup.onHandOff();
    expect(last()).toMatchObject({ busy: false, error: "no_permission" });
    expect(d.handOffs).toBe(0);
  });

  it("hands off with the length, localStorage choice and session the person set", async () => {
    let asked: unknown;
    const d = deps({
      handOff: async (ask) => {
        asked = ask;
        return ok(outcome);
      },
    });
    const { popup } = await started(d);
    popup.onToggle({ kind: "ttl", seconds: 28800 });
    popup.onToggle({ kind: "localStorage" });
    popup.onSession("other");
    await popup.onHandOff();
    expect(asked).toEqual({ tabId: 7, session: "other", ttlSeconds: 28800, includeLocalStorage: true });
  });
});
