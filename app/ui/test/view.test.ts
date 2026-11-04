import { describe, expect, it } from "vitest";
import type { Item } from "../src/api.js";
import { alone, asking, closeForm, edit, go, hide, initial, openForm, select, show } from "../src/state.js";
import { editorValues } from "../src/views/editor.js";
import { formValues } from "../src/views/form.js";
import { esc, render } from "../src/view.js";
import { handOffWord } from "../src/words.js";

const github: Item = {
  name: "github", kind: "login", domain: "credentials", status: "active", source: "user",
  description: null, fields: ["password", "username"],
  placeholders: ["{{secret:github.password}}", "{{secret:github.username}}"],
  plain_values: {}, origins: ["https://github.com"], tags: [], expired: false, tampered: false,
};
const withItems = { ...initial, view: "items" as const, items: [github, { ...github, name: "other" }] };

describe("revealed values", () => {
  it("show only for the field asked, and leave the page on any move away", () => {
    const shown = show(select(withItems, "github"), { item: "github", field: "password", value: "pw-SECRET-1" });
    expect(render(shown)).toContain("pw-SECRET-1");
    expect(render(shown).match(/pw-SECRET-1/g)).toHaveLength(1);
    for (const away of [hide(shown), select(shown, "other"), go(shown, "home"), select(shown, "github")]) {
      expect(away.revealed).toBeNull();
      expect(render(away)).not.toContain("pw-SECRET-1");
    }
  });

  it("are not kept when the selection moved before the value arrived", () => {
    const moved = select(withItems, "other");
    expect(show(moved, { item: "github", field: "password", value: "late" }).revealed).toBeNull();
  });
});

describe("rendering", () => {
  it("escapes every text from the vault and the agent", () => {
    const evil = { ...github, name: "<img src=x onerror=alert(1)>", tags: ["<b>"] };
    const html = render(select({ ...withItems, items: [evil] }, evil.name));
    expect(html).not.toContain("<img src=x");
    expect(html).toContain(esc("<img src=x onerror=alert(1)>"));
    const reason = { id: "r1", agent: "codex", kind: "card", name: "visa", origin: null, reason: "<script>x</script>" };
    expect(render({ ...initial, view: "inbox", inbox: { requests: [reason], drafts: [] } })).not.toContain("<script>");
  });

  it("marks tampered items and offers no reveal for them", () => {
    const html = render(select({ ...withItems, items: [{ ...github, tampered: true }] }, "github"));
    expect(html).toContain("변조 의심");
    expect(html).not.toContain('data-action="reveal"');
  });

  it("shows plain values without a reveal button", () => {
    const env = { ...github, name: "app", fields: ["PORT"], placeholders: ["{{secret:app.PORT}}"], plain_values: { PORT: "8081" } };
    const html = render(select({ ...withItems, items: [env] }, "app"));
    expect(html).toContain("8081");
    expect(html).not.toContain('data-action="reveal"');
  });

  it("asks for vault init instead of making a key when there is no vault", () => {
    const html = render({ ...initial, overview: { vault_ready: false, sessions_open: 0, requests_pending: 0, drafts: 0, auth: "none" } });
    expect(html).toContain("keyharbor vault init");
  });
});

describe("the secure input form", () => {
  const form = {
    id: "r1", agent: "codex", kind: "card", name: "visa", origin: "https://shop.example", reason: "checkout",
    fields: [{ name: "number", secret: true }, { name: "holder", secret: false }], free_fields: false, custom_fields: false,
  };

  it("in its own window shows only the form, and says so when the request is gone", () => {
    const html = render(alone(initial, form));
    expect(html).toContain("codex");
    expect(html).not.toContain('aria-label="KeyHarbor"');
    expect(html).toContain("detail alone");
    expect(render(alone(initial, null))).toContain("이 요청은 이미 끝났습니다");
  });

  it("can be opened in its own window from the inbox", () => {
    const waiting = { ...initial, view: "inbox" as const, inbox: { requests: [{ id: "r1", agent: "codex", kind: "card", name: "visa", origin: null, reason: "checkout" }], drafts: [] } };
    expect(render(waiting)).toContain('data-action="open-window" data-id="r1"');
  });

  it("hides secret fields and turns off remembering and completion", () => {
    const html = render(openForm(initial, form));
    expect(html).toMatch(/name="number" type="password"/);
    expect(html).toMatch(/name="holder" type="text"/);
    expect(html.match(/autocomplete="off"/g)?.length).toBe(2);
    expect(html).toContain("codex");
    expect(html).toContain("이 값은 에이전트에게 보이지 않습니다");
  });

  it("is gone from the page once closed or left", () => {
    const opened = openForm(initial, form);
    expect(render(closeForm(opened))).not.toContain('data-form="r1"');
    expect(render(go(opened, "home"))).not.toContain('data-form="r1"');
  });

  it("reads KEY=VALUE lines for env items", () => {
    const data = new FormData();
    data.set("__env", "API_KEY=sk-1\nPORT=8081\nnot a line");
    expect(formValues(data)).toEqual({ API_KEY: "sk-1", PORT: "8081" });
  });
});

describe("weakening changes", () => {
  it("show their question in a dialog until answered", () => {
    const asked = asking(initial, { question: "policy allow codex item github", retry: async () => undefined });
    expect(render(asked)).toContain("policy allow codex item github");
    expect(render(asking(asked, null))).not.toContain("alertdialog");
    expect(go(asked, "home").ask).toBeNull();
  });
});

describe("copying and sessions", () => {
  it("offers value and placeholder copy, but no value copy for tampered or plain fields", () => {
    const html = render(select(withItems, "github"));
    expect(html).toContain('data-action="copy" data-name="github" data-field="password"');
    expect(html).toContain('data-action="copy-placeholder" data-text="{{secret:github.password}}"');
    const tampered = render(select({ ...withItems, items: [{ ...github, tampered: true }] }, "github"));
    expect(tampered).not.toContain('data-action="copy"');
  });

  it("shows open, stop, or recovery behind a checkbox", () => {
    const session = (state: string) => ({
      id: "s1", name: "work", state, cdp: false, description: null, browser: null, protection: null, failure: null, sites: [],
      live: state === "running", recoverable: state === "recovery_required",
    });
    const view = (state: string) => render({ ...initial, view: "sessions", sessions: [session(state)] });
    expect(view("stopped")).toContain('data-action="open-session"');
    expect(view("running")).toContain('data-action="stop-session"');
    expect(view("recovery_required")).toMatch(/data-closed="s1".*data-action="recover"/s);
  });
});

describe("the item forms", () => {
  const kinds = [
    { kind: "login", domain: "credentials", fields: [["username", false], ["password", true]] as [string, boolean][], free_fields: false, custom_fields: false, generates: "password" },
    { kind: "env", domain: "developer", fields: [] as [string, boolean][], free_fields: true, custom_fields: false, generates: null },
  ];

  it("offer the kind's fields, password inputs for secrets, and generation for logins", () => {
    const html = render(edit({ ...withItems, kinds }, { mode: "new", kind: "login" }));
    expect(html).toMatch(/name="v:password" type="password"/);
    expect(html).toMatch(/name="v:username" type="text"/);
    expect(html).toContain('name="generate"');
    const env = render(edit({ ...withItems, kinds }, { mode: "new", kind: "env" }));
    expect(env).toContain('name="__env"');
    expect(env).not.toContain('name="generate"');
  });

  it("edit an item with its sites filled in and no value", () => {
    const opened = edit(select(withItems, "github"), { mode: "edit", name: "github" });
    const html = render(opened);
    expect(html).toContain("https://github.com</textarea>");
    expect(html).toContain("비우면 그대로");
    expect(edit(opened, null).editor).toBeNull();
    expect(select(opened, "other").editor).toBeNull();
  });

  it("read field values and sites from the form", () => {
    const data = new FormData();
    data.set("v:password", "pw");
    data.set("origins", "https://a.example\n\n https://b.example ");
    expect(editorValues(data)).toEqual({ values: { password: "pw" }, origins: ["https://a.example", "https://b.example"] });
  });
});

describe("what Rust decides, the window only shows", () => {
  const event = (kind: string, denied: boolean) => ({ at: "2026-10-29T05:02:00Z", agent: "codex", kind, denied, item: null, origin: null, result: "x" });
  const audit = (e: ReturnType<typeof event>) => render({ ...initial, view: "audit", audit: [e] });

  it("marks a denial by the record's flag, the one the notices use (J-200)", () => {
    expect(audit(event("secret_denied_suppressed", true))).toContain('class="kh-badge denied"');
    expect(audit(event("future_denied_kind", false))).not.toContain('class="kh-badge denied"');
  });

  it("words a site hand-off by its outcome", () => {
    expect(handOffWord({ outcome: "handed_off", cookies: 3 }, "work")).toBe("쿠키 3개를 work 세션에 넘겼습니다");
    expect(handOffWord({ outcome: "no_session" }, "work")).toBe("실행 중인 work 세션이 없습니다");
  });

  it("offers generation only for the field a kind names", () => {
    const kinds = [{ kind: "secret", domain: "developer", fields: [["value", true]] as [string, boolean][], free_fields: false, custom_fields: false, generates: "value" }];
    expect(render(edit({ ...initial, view: "items", kinds }, { mode: "new", kind: "secret" }))).toContain("값 생성");
  });
});
