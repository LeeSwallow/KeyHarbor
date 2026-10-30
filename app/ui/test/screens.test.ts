import { describe, expect, it } from "vitest";
import { asking, go, initial } from "../src/state.js";
import { render } from "../src/view.js";

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
