import { describe, expect, it } from "vitest";
import type { Item } from "../src/api.js";
import { go, hide, initial, select, show } from "../src/state.js";
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
