// The screens for job applications and ego lite (#123): sessions show their browser and why
// one did not start, a stopped session can be removed with undo, the identity form groups its
// fields and takes fields the person names, and the settings hold the browser and upload folders.

import { describe, expect, it } from "vitest";
import type { KindForm, Session, Setting } from "../src/api.js";
import { initial, removingSession, undoRemoveSession } from "../src/state.js";
import { render } from "../src/view.js";
import { editor, editorValues } from "../src/views/editor.js";
import { settings } from "../src/views/settings.js";

const session = (patch: Partial<Session>): Session => ({
  id: "s1", name: "jobs", state: "running", cdp: false, description: null, sites: [],
  browser: "chrome", protection: "owned", failure: null, live: true, recoverable: false, ...patch,
});
const sessionsScreen = (list: Session[], egoLite = false) =>
  render({ ...initial, view: "sessions", sessions: list, browsers: { ego_lite: egoLite } });
const shown = (html: string) => html.replace(/<[^>]*>/g, " ");

describe("the sessions screen", () => {
  it("session_card_shows_browser_and_protection", () => {
    const badge = '<span class="kh-badge neutral">Chrome</span>';
    const chrome = sessionsScreen([session({})]);
    expect(chrome).toContain(badge);
    expect(shown(chrome)).not.toContain("보호 낮음");
    const ego = sessionsScreen([session({ browser: "ego-lite", protection: "shared" })]);
    expect(shown(ego)).toContain("ego lite");
    expect(shown(ego)).toContain("보호 낮음");
    expect(ego).toContain("넣은 값을 읽을 수 있습니다");
    // A stopped session's last browser is history, not a promise about the next run.
    expect(sessionsScreen([session({ state: "stopped", live: false })])).not.toContain(badge);
  });

  it("says why a session did not start", () => {
    const failed = session({ state: "failed", live: false, recoverable: true, failure: "ego lite is not available" });
    const text = shown(sessionsScreen([failed]));
    expect(text).toContain("시작하지 못했습니다: ego lite is not available");
    expect(sessionsScreen([failed])).toContain('data-action="recover"');
  });

  it("offers a browser to open in only when ego lite is here", () => {
    const stopped = [session({ state: "stopped", live: false })];
    expect(sessionsScreen(stopped, false)).not.toContain('data-browser="ego-lite"');
    const both = sessionsScreen(stopped, true);
    expect(both).toContain('data-action="open-session" data-id="s1" data-browser="ego-lite"');
    expect(both).toContain('data-action="open-session" data-id="s1" data-browser="chrome"');
    expect(both).toContain('data-action="open-session" data-id="s1">');
  });

  it("stopped_session_can_be_removed_with_undo", () => {
    const list = [session({ state: "stopped", live: false }), session({ id: "s2", name: "work", state: "stopped", live: false })];
    const base = { ...initial, view: "sessions" as const, sessions: list };
    expect(render(base)).toContain('data-action="remove-session" data-id="s1" data-name="jobs"');
    expect(render({ ...base, sessions: [session({})] })).not.toContain('data-action="remove-session"');
    const gone = removingSession(base, "s1", "jobs");
    const html = render(gone);
    expect(html).not.toContain('data-session-drop="jobs"');
    expect(html).toContain('data-session-drop="work"');
    expect(html).toContain("jobs 세션을(를) 지웠습니다");
    expect(html).toContain('data-action="undo-remove-session"');
    expect(render(undoRemoveSession(gone))).toContain('data-session-drop="jobs"');
  });
});

const identity: KindForm = {
  kind: "identity", domain: "personal", free_fields: false, custom_fields: true, generates: null,
  fields: [
    ["full_name", false], ["given_name", false], ["family_name", false], ["email", false], ["phone", false],
    ["birth_date", false], ["english_name", false], ["hanja_name", false], ["gender", false], ["nationality", false],
    ["emergency_phone", false], ["military_service", true], ["disability", true], ["veterans", true], ["note", true],
  ],
};
const login: KindForm = { kind: "login", domain: "credentials", free_fields: false, custom_fields: false, generates: "password", fields: [["username", false], ["password", true]] };

describe("the identity form", () => {
  it("identity_editor_groups_fields_and_adds_custom", () => {
    const html = editor({ mode: "new", kind: "identity" }, [identity, login], []);
    for (const group of ["기본", "연락처", "병역·보훈", "직접 추가한 항목"]) {
      expect(html).toContain(`<legend>${group}</legend>`);
    }
    // Service status is sensitive: a password input, nothing shown back.
    expect(html).toContain('name="v:military_service" type="password"');
    expect(html).toContain('name="v:english_name" type="text"');
    expect(html).toContain('name="cn:0"');
    expect(html).toContain('data-action="add-custom-field"');
    expect(editor({ mode: "new", kind: "login" }, [identity, login], [])).not.toContain("직접 추가한 항목");
  });

  it("reads the fields the person named", () => {
    const data = new FormData();
    data.set("v:full_name", "홍길동");
    data.set("cn:0", " school ");
    data.set("cv:0", "서울대학교");
    data.set("cn:1", "");
    data.set("cv:1", "ignored");
    data.set("cn:2", "gpa");
    data.set("cv:2", "");
    expect(editorValues(data).values).toEqual({ full_name: "홍길동", school: "서울대학교" });
  });

  it("puts an item's own fields with the ones it can add", () => {
    const item = {
      name: "me", kind: "identity", domain: "personal", status: "active", source: "user", description: null,
      fields: ["full_name", "school"], placeholders: [], plain_values: {}, origins: [], tags: [], expired: false, tampered: false,
    };
    const html = editor({ mode: "edit", name: "me" }, [identity], [item]);
    const custom = html.slice(html.indexOf("직접 추가한 항목"));
    expect(custom).toContain('name="v:school"');
    expect(html.slice(0, html.indexOf("직접 추가한 항목"))).toContain('name="v:full_name"');
  });
});

const setting = (key: string, value: string, def = ""): Setting => ({ key, value, default: def, weakening: { when: "never" } });

describe("the settings screen", () => {
  it("settings_lists_upload_dirs_and_default_browser", () => {
    const list = [
      setting("browser", "auto", "auto"),
      setting("upload_dirs", "/Users/me/Documents/Recruiting:/Users/me/Pictures", ""),
      setting("upload_max_files", "10", "10"),
    ];
    const html = settings(list, { ego_lite: false });
    expect(html).toContain("<h3>브라우저</h3>");
    expect(html).toContain("<h3>파일 올리기</h3>");
    expect(html).toContain('<button type="button" class="is-on" data-action="set-setting" data-key="browser" data-value="auto">자동</button>');
    expect(shown(html)).toContain("지금은 ego lite를 찾지 못했습니다");
    expect(html).toContain("<code>/Users/me/Documents/Recruiting</code>");
    // Taking a folder out sends the rest of the list.
    expect(html).toContain('data-key="upload_dirs" data-value="/Users/me/Pictures">빼기');
    expect(html).toContain('data-setting="upload_dirs" data-append="/Users/me/Documents/Recruiting:/Users/me/Pictures"');
    expect(shown(settings(list, { ego_lite: true }))).toContain("지금 ego lite가 켜져 있습니다");
  });

  it("shows no folders as nothing allowed", () => {
    expect(shown(settings([setting("upload_dirs", "")]))).toContain("허용한 폴더가 없습니다");
  });
});

describe("a failed session", () => {
  it("shows no browser: the run never started", () => {
    const failed = session({ state: "failed", live: false, recoverable: true, failure: "x" });
    expect(sessionsScreen([failed])).not.toContain('<span class="kh-badge neutral">Chrome</span>');
  });
});
