import { describe, expect, it } from "vitest";
import type { Item } from "../src/api.js";
import { closePalette, initial, openPalette, query } from "../src/state.js";
import { paletteCommands, palette } from "../src/views/palette.js";
import { render } from "../src/view.js";

const item = (name: string): Item => ({
  name, kind: "login", domain: "credentials", status: "active", source: "user",
  description: null, fields: ["password"], placeholders: [`{{secret:${name}.password}}`],
  plain_values: {}, origins: [], tags: [], expired: false, tampered: false,
});
const withItems = { ...initial, items: [item("github"), item("gitlab"), item("bank")] };

describe("the command palette", () => {
  it("lists every screen, a new item, and every item", () => {
    const labels = paletteCommands(withItems, "").map((c) => c.label);
    expect(labels).toContain("확인 대기로 가기");
    expect(labels).toContain("새 항목");
    expect(labels).toContain("github 열기");
    expect(labels).toHaveLength(7 + 1 + 3);
  });

  it("narrows by what was typed, ignoring case and spaces", () => {
    expect(paletteCommands(withItems, "GIT").map((c) => c.label)).toEqual(["github 열기", "gitlab 열기"]);
    expect(paletteCommands(withItems, "새 항").map((c) => c.label)).toEqual(["새 항목"]);
    expect(paletteCommands(withItems, "zzz")).toEqual([]);
  });

  it("opens, keeps the query, and closes", () => {
    const open = query(openPalette(withItems), "bank");
    expect(open.palette).toEqual({ query: "bank" });
    expect(render(open)).toContain('data-action="run"');
    expect(render(open)).toContain("bank 열기");
    expect(closePalette(open).palette).toBeNull();
    expect(render(closePalette(open))).not.toContain('data-palette');
  });

  it("escapes what it shows", () => {
    const html = palette({ query: "<b>" }, [{ label: "<i>", run: { kind: "new-item" } }]);
    expect(html).not.toContain("<i>");
    expect(html).not.toContain('value="<b>"');
  });
});
