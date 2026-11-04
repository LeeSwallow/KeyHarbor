import { describe, expect, it } from "vitest";
import type { Setting, Weakening } from "../src/api.js";
import { settings } from "../src/views/settings.js";

const never: Weakening = { when: "never" };
const all: Setting[] = [
  { key: "audit_enabled", value: "true", default: "true", weakening: { when: "set_to", value: "false" } },
  { key: "capture_enabled", value: "false", default: "false", weakening: { when: "any_change" } },
  { key: "default_item_ttl", value: "none", default: "none", weakening: never },
  { key: "draft_ttl", value: "45d", default: "7d", weakening: never },
  { key: "clipboard_clear_seconds", value: "30", default: "30", weakening: { when: "above", than: "30" } },
  { key: "notify_requests", value: "true", default: "true", weakening: never },
  { key: "notify_handoffs", value: "true", default: "true", weakening: never },
  { key: "notify_drafts", value: "false", default: "true", weakening: never },
  { key: "notify_denials", value: "true", default: "true", weakening: never },
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
    const other = settings([{ key: "future_thing", value: "x", default: "y", weakening: never }]);
    expect(other).toContain('data-setting="future_thing"');
    expect(html).toContain("macOS 인증");
    expect(html).toMatch(/<button[^>]*disabled[^>]*>macOS 인증/);
  });

  it("words the weakening limit with the number Rust sends, not one of its own", () => {
    expect(html).toContain("30초보다 길게 하면 확인합니다. 다른 것을 복사했으면 지우지 않습니다.");
    const raised = settings([{ ...all[4]!, weakening: { when: "above", than: "45" } }]);
    expect(raised).toContain("45초보다 길게 하면 확인합니다.");
    expect(raised).not.toContain("30초보다");
    const files = settings([{ key: "upload_max_files", value: "10", default: "10", weakening: { when: "above", than: "10" } }]);
    expect(files).toContain("10개보다 많게 하면 확인합니다.");
  });

  it("gathers the waits and limits under 고급, in words, with Rust's weakening limit (J-274)", () => {
    const advanced = settings([
      { key: "session_start_seconds", value: "15", default: "15", weakening: never },
      { key: "session_stop_seconds", value: "15", default: "15", weakening: never },
      { key: "browser_endpoint_seconds", value: "15", default: "15", weakening: never },
      { key: "browser_call_seconds", value: "45", default: "30", weakening: never },
      { key: "request_ttl_hours", value: "24", default: "24", weakening: { when: "above", than: "24" } },
      { key: "max_drafts", value: "20", default: "20", weakening: { when: "above", than: "20" } },
    ]);
    expect(advanced).toContain("<h3>고급</h3>");
    expect(advanced).not.toContain("<h3>기타</h3>");
    expect(advanced).toContain("세션 시작을 기다리는 시간");
    expect(advanced).toMatch(/class="is-on"[^>]*data-key="session_start_seconds" data-value="15">15초<\/button>/);
    expect(advanced).toContain('data-key="session_start_seconds" data-value="120">2분</button>');
    expect(advanced).toMatch(/class="is-on"[^>]*data-key="browser_call_seconds" data-value="45">45<\/button>/);
    expect(advanced).toContain("24시간보다 길게 하면 확인합니다.");
    expect(advanced).toContain("20개보다 많게 하면 확인합니다.");
    expect(advanced).not.toMatch(/>\s*(session_start_seconds|max_drafts|request_ttl_hours)\s*</);
  });

  it("marks the method the app's authenticator names, and marks none before it is known", () => {
    const none = settings(all, { ego_lite: false }, "none");
    expect(none).toMatch(/<button type="button" class="is-on">없음<\/button>/);
    expect(none).toContain("인증 없음: 이 Mac 사용자 계정으로 접근할 수 있는 누구나 금고를 볼 수 있습니다.");
    const unknown = settings(all, { ego_lite: false }, null);
    expect(unknown).not.toContain('class="is-on">없음');
    expect(unknown).not.toContain("인증 없음:");
  });

  it("offers agent one-time codes as one switch under 보안, off by default (#206)", () => {
    const otp = settings([{ key: "agent_otp", value: "false", default: "false", weakening: { when: "set_to", value: "true" } }]);
    expect(otp).toContain("<h3>보안</h3>");
    expect(otp).toContain("에이전트가 OTP 코드 입력");
    expect(otp).toContain('role="switch" aria-checked="false" data-action="set-setting" data-key="agent_otp" data-value="true"');
    expect(otp).toContain("2단계 인증이 사실상 1단계가 됩니다");
  });
});
