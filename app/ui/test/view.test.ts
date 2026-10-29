import { describe, expect, it } from "vitest";
import type { Item } from "../src/api.js";
import { asking, closeForm, go, hide, initial, openForm, select, show } from "../src/state.js";
import { formValues } from "../src/views/form.js";
import { esc, render } from "../src/view.js";

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
    const html = render({ ...initial, overview: { vault_ready: false, sessions_open: 0, requests_pending: 0, drafts: 0 } });
    expect(html).toContain("keyharbor vault init");
  });
});

describe("the secure input form", () => {
  const form = {
    id: "r1", agent: "codex", kind: "card", name: "visa", origin: "https://shop.example", reason: "checkout",
    fields: [{ name: "number", secret: true }, { name: "holder", secret: false }], free_fields: false,
  };

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
    const session = (state: string) => ({ id: "s1", name: "work", state, cdp: false, description: null, sites: [] });
    const view = (state: string) => render({ ...initial, view: "sessions", sessions: [session(state)] });
    expect(view("stopped")).toContain('data-action="open-session"');
    expect(view("running")).toContain('data-action="stop-session"');
    expect(view("recovery_required")).toMatch(/data-closed="s1".*data-action="recover"/s);
  });
});
