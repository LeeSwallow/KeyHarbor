// A click goes to the handler its `data-action` names (#182). The window is mounted on a
// stand-in root and port, with no browser: only what the click path touches is faked.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Api, Item } from "../src/api.js";
import { mount } from "../src/main.js";

/// What `closest` and `instanceof` see: the button itself, with its `data-*` attributes.
class FakeElement {
  constructor(readonly dataset: Record<string, string>) {}
  closest() {
    return this;
  }
}

const github: Item = {
  name: "github", kind: "login", domain: "credentials", status: "active", source: "user", description: null,
  fields: ["password"], placeholders: [], plain_values: {}, origins: [], tags: [], expired: false, tampered: false,
};
const answers: Record<string, unknown> = {
  overview: null, inbox: { requests: [], drafts: [] }, sessions: [], items: [github], audit: [], handoffs: [],
  settings: [], policies: [], kinds: [], browsers: { ego_lite: false },
};

/// A port that records every command and answers with `answers`.
function fakeApi(calls: string[]): Api {
  const handler: ProxyHandler<object> = {
    get: (_target, command) => () => {
      calls.push(String(command));
      return Promise.resolve(answers[String(command)]);
    },
  };
  return new Proxy({}, handler) as Api;
}

async function mounted() {
  const listeners: Record<string, (event: unknown) => void> = {};
  const root = {
    innerHTML: "",
    querySelector: () => null,
    addEventListener: (type: string, listener: (event: unknown) => void) => {
      listeners[type] = listener;
    },
    ownerDocument: { addEventListener: () => undefined },
  };
  const calls: string[] = [];
  const window = mount(root as unknown as HTMLElement, fakeApi(calls));
  await new Promise((done) => setTimeout(done, 0));
  const click = (dataset: Record<string, string>) => listeners.click?.({ target: new FakeElement(dataset) });
  return { window, calls, click };
}

describe("a click", () => {
  beforeEach(() => {
    vi.stubGlobal("Element", FakeElement);
    vi.stubGlobal("HTMLElement", FakeElement);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("on an action no handler has does nothing", async () => {
    const { window, calls, click } = await mounted();
    const before = window.state();
    const called = calls.length;
    for (const action of ["no-such-action", "toString", "__proto__", "constructor"]) click({ action, name: "github" });
    expect(window.state()).toBe(before);
    expect(calls).toHaveLength(called);
  });

  it("to a screen that does not exist goes nowhere", async () => {
    const { window, click } = await mounted();
    const before = window.state();
    click({ action: "go", view: "nowhere" });
    expect(window.state()).toBe(before);
  });

  it("runs the handler its action names", async () => {
    const { window, click } = await mounted();
    click({ action: "go", view: "items" });
    click({ action: "select", name: "github" });
    expect(window.state().view).toBe("items");
    expect(window.state().selected).toBe("github");
  });

  it("starts no new item form before the kinds have loaded", async () => {
    const { window, click } = await mounted();
    const before = window.state();
    click({ action: "new-item" });
    expect(window.state()).toBe(before);
  });
});
