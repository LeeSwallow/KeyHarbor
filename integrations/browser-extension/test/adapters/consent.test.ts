import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { NotificationOptionsWire } from "../../src/adapters/webext/api.js";
import { NOTIFICATION_ICON, webextNotifier } from "../../src/adapters/webext/sources/consent.js";
import { fakeBrowser } from "../support/fake-browser.js";

function fakeNotifications() {
  const api = fakeBrowser();
  const created: Array<{ id: string; options: NotificationOptionsWire }> = [];
  const cleared: string[] = [];
  const on = { clicked: (_id: string) => undefined as void, button: (_id: string, _b: number) => undefined as void, closed: (_id: string) => undefined as void };
  api.notifications = {
    create: async (id, options) => {
      created.push({ id, options });
      return id;
    },
    clear: async (id) => {
      cleared.push(id);
      return true;
    },
    onClicked: { addListener: (l) => void (on.clicked = l) },
    onButtonClicked: { addListener: (l) => void (on.button = l) },
    onClosed: { addListener: (l) => void (on.closed = l) },
  };
  return { api, created, cleared, on };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("the hand-off notification", () => {
  it("shows a button in Korean and answers yes when it is pressed", async () => {
    vi.useFakeTimers();
    const fake = fakeNotifications();
    const answer = webextNotifier(fake.api, { gesture: "button", lang: "ko" }).askHandOff("https://github.com", "work");
    await Promise.resolve();
    const shown = fake.created[0];
    expect(shown?.options.title).toBe("KeyHarbor: 사이트 넘기기");
    expect(shown?.options.message).toBe("https://github.com의 로그인을 work 세션에 넘길까요?");
    expect(shown?.options.buttons).toEqual([{ title: "넘기기" }]);
    expect(shown?.options.iconUrl).toBe(`chrome-extension://test/${NOTIFICATION_ICON}`);
    fake.on.button(shown?.id ?? "", 0);
    await expect(answer).resolves.toBe(true);
    expect(fake.cleared).toEqual([shown?.id]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("is clicked in English when there are no buttons", async () => {
    const fake = fakeNotifications();
    const answer = webextNotifier(fake.api, { gesture: "click", lang: "en" }).askHandOff("https://github.com", "work");
    await Promise.resolve();
    const shown = fake.created[0];
    expect(shown?.options.message).toBe("Hand your https://github.com login to the work session? Click this notification to hand it off.");
    expect(shown?.options.buttons).toBeUndefined();
    fake.on.clicked(shown?.id ?? "");
    await expect(answer).resolves.toBe(true);
  });

  it("answers no when dismissed, and once only", async () => {
    const fake = fakeNotifications();
    const answer = webextNotifier(fake.api, { gesture: "click", lang: "en" }).askHandOff("https://github.com", "work");
    await Promise.resolve();
    const id = fake.created[0]?.id ?? "";
    fake.on.closed(id);
    fake.on.clicked(id);
    await expect(answer).resolves.toBe(false);
    expect(fake.cleared).toEqual([id]);
  });

  it("answers no when nobody answers in time", async () => {
    vi.useFakeTimers();
    const fake = fakeNotifications();
    const answer = webextNotifier(fake.api, { gesture: "button", lang: "en" }, 1000).askHandOff("https://github.com", "work");
    await vi.advanceTimersByTimeAsync(1000);
    await expect(answer).resolves.toBe(false);
    expect(fake.cleared).toHaveLength(1);
  });

  it("uses an icon the manifest lists", () => {
    const manifest = JSON.parse(readFileSync(new URL("../../manifest/base.json", import.meta.url), "utf8")) as { icons: Record<string, string> };
    expect(Object.values(manifest.icons)).toContain(NOTIFICATION_ICON);
  });
});
