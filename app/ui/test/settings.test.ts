import { describe, expect, it } from "vitest";
import type { Setting } from "../src/api.js";
import { settings } from "../src/views/settings.js";

const all: Setting[] = [
  { key: "audit_enabled", value: "true", default: "true" },
  { key: "capture_enabled", value: "false", default: "false" },
  { key: "default_item_ttl", value: "none", default: "none" },
  { key: "draft_ttl", value: "45d", default: "7d" },
  { key: "clipboard_clear_seconds", value: "30", default: "30" },
  { key: "notify_requests", value: "true", default: "true" },
  { key: "notify_handoffs", value: "true", default: "true" },
  { key: "notify_drafts", value: "false", default: "true" },
  { key: "notify_denials", value: "true", default: "true" },
];

describe("the settings screen", () => {
  const html = settings(all);

  it("names settings in words, grouped, and never shows the stored keys as text", () => {
    for (const heading of ["보안", "새 항목과 draft", "클립보드", "알림"]) expect(html).toContain(`<h3>${heading}</h3>`);
    expect(html).toContain("사용 기록 남기기");
    expect(html).toContain("복사한 값 자동 지우기");
    expect(html).not.toMatch(/>\s*(audit_enabled|notify_drafts|true|false)\s*</);
  });

  it("turns on/off settings into switches that send the opposite value", () => {
    expect(html).toContain('role="switch" aria-checked="true" data-action="set-setting" data-key="audit_enabled" data-value="false"');
    expect(html).toContain('class="kh-toggle" role="switch" aria-checked="false" data-action="set-setting" data-key="notify_drafts" data-value="true"');
  });

  it("offers choices, marks the current one, and keeps a value set elsewhere", () => {
    expect(html).toMatch(/class="is-on"[^>]*data-key="clipboard_clear_seconds" data-value="30"/);
    expect(html).toMatch(/class="is-on"[^>]*data-key="draft_ttl" data-value="45d">45일<\/button>/);
    expect(html).toContain('data-key="default_item_ttl" data-value="90d">90일</button>');
  });

  it("shows a setting it does not know as a plain field, and the coming sign-in methods as disabled", () => {
    const other = settings([{ key: "future_thing", value: "x", default: "y" }]);
    expect(other).toContain('data-setting="future_thing"');
    expect(html).toContain("macOS 인증");
    expect(html).toMatch(/<button[^>]*disabled[^>]*>macOS 인증/);
  });
});
