import { describe, expect, it } from "vitest";
import { asking, edit, go, initial, select } from "../src/state.js";
import { render } from "../src/view.js";
import { droppedUrl } from "../src/drop.js";

/// Text between tags, so attribute values (data-*, class) do not count as shown words.
const shownText = (html: string) => html.replace(/<[^>]*>/g, " ");

const state = {
  ...initial,
  inbox: {
    requests: [{ id: "r1", agent: "codex", kind: "card", name: "visa", origin: "https://shop.example", reason: "checkout" }],
    drafts: [{ name: "vercel", kind: "login", source: "generated", origins: [] }],
  },
  sessions: [{ id: "s1", name: "work", state: "running", cdp: false, description: null, sites: [{ origin: "https://github.com", state: "logged_in", item: null, checked_at: "" }] }],
  policies: [{ agent: "codex", resource: "domain", target: "payment", effect: "deny" }],
  audit: [{ at: "2026-10-29T05:02:00Z", agent: "codex", kind: "secret_denied", item: "bank", origin: null, result: "origin" }],
};

describe("every screen", () => {
  it("says states and kinds in words, never the stored values", () => {
    const screens = {
      inbox: ["카드", "생성됨", "확인 대기"],
      sessions: ["실행 중", "로그인됨", "세션"],
      policies: ["도메인", "결제", "거부"],
      audit: ["사용 거부", "10-29 05:02"],
    } as const;
    for (const [view, words] of Object.entries(screens)) {
      const text = shownText(render(go(state, view as keyof typeof screens)));
      for (const w of words) expect(text, view).toContain(w);
      for (const raw of ["running", "logged_in", "generated", "secret_denied", " card ", "payment", " deny "]) {
        expect(text, `${view} shows ${raw}`).not.toContain(raw);
      }
    }
  });

  it("gives every button a design-system variant, with one primary action per request", () => {
    const html = render(go(state, "inbox"));
    for (const b of html.match(/<button[^>]*class="kh-btn[^"]*"/g) ?? []) expect(b).toMatch(/kh-btn (primary|secondary|ghost|danger)/);
    expect(html.match(/kh-btn primary[^>]*data-action="open-form"/g)).toHaveLength(1);
  });

  it("asks with a danger confirm button", () => {
    const html = render(asking(state, { question: "q", retry: async () => undefined }));
    expect(html).toMatch(/kh-btn danger[^>]*data-action="ask-confirm"/);
  });
});

describe("the home screen", () => {
  it("counts what waits the same way as the side bar", () => {
    const waiting = {
      ...state,
      overview: { vault_ready: true, sessions_open: 1, requests_pending: 1, drafts: 1 },
      handoffs: [{ session: "s1", session_name: "work", request_id: "h1", origin: null }],
    };
    const html = render(go(waiting, "home"));
    expect(html).toMatch(/확인 대기<\/span><strong>3<\/strong>/);
    expect(html).toMatch(/확인 대기<span class="kh-count">3<\/span>/);
  });
});

describe("icons", () => {
  const icons = { home: "house", inbox: "inbox", sessions: "app-window", items: "key-round", audit: "scroll-text", policies: "shield-check", settings: "settings" };

  it("mark every tab, every screen title, and the app at the top of the side bar", () => {
    for (const [view, name] of Object.entries(icons)) {
      const html = render(go(state, view as keyof typeof icons));
      expect(html, view).toMatch(new RegExp(`data-view="${view}"><span class="kh-i sm i-${name}"`));
      expect(html, view).toMatch(new RegExp(`<h2><span class="kh-i i-${name}"[^>]*></span>`));
      expect(html).toContain('<img class="brand-icon" src="app-icon.svg" alt="">');
    }
  });
});

describe("renaming an item", () => {
  const github = {
    name: "github", kind: "login", domain: "credentials", status: "active", source: "user", description: null,
    fields: ["password"], placeholders: ["{{secret:github.password}}"], plain_values: {}, origins: [], tags: [],
    expired: false, tampered: false,
  };
  const opened = select({ ...initial, view: "items" as const, items: [github] }, "github");

  it("is offered on the item, and warns that its placeholders change", () => {
    expect(render(opened)).toContain('data-action="rename-item" data-name="github"');
    const html = render(edit(opened, { mode: "rename", name: "github" }));
    expect(html).toContain('data-editor="rename" data-name="github"');
    expect(html).toContain("{{secret:github.password}}");
    expect(html).toContain("에이전트 설정에 옛 이름이 있으면 고쳐야 합니다");
  });
});

describe("dropping a site on a session", () => {
  it("makes every session card a drop target and says how to use it", () => {
    const html = render(go(state, "sessions"));
    expect(html).toContain('data-session-drop="work"');
    expect(html).toContain("끌어다 놓으면");
  });

  it("shows how a drop ended", () => {
    const html = render({ ...go(state, "sessions"), notice: "쿠키 3개를 work 세션에 넘겼습니다" });
    expect(html).toContain('class="kh-toast ok"');
    expect(html).toContain("쿠키 3개를 work 세션에 넘겼습니다");
  });
});

describe("the URL taken from a drop", () => {
  it("is the first http(s) line of a uri-list, or nothing", () => {
    expect(droppedUrl("# comment\nhttps://github.com/settings\nhttps://x.example")).toBe("https://github.com/settings");
    expect(droppedUrl("file:///etc/passwd")).toBeNull();
    expect(droppedUrl("")).toBeNull();
  });
});
