import { describe, expect, it } from "vitest";
import type { Item } from "../src/api.js";
import { filterItems, initial, search } from "../src/state.js";
import { render } from "../src/view.js";

const item = (name: string, kind: string, extra: Partial<Item> = {}): Item => ({
  name, kind, domain: "credentials", status: "active", source: "user", description: null,
  fields: [], placeholders: [], plain_values: {}, origins: [], tags: [], expired: false, tampered: false, ...extra,
});
const items = [
  item("github", "login", { origins: ["https://github.com"] }),
  item("myapp/prod/db", "env", { tags: ["prod"] }),
  item("vercel", "login", { status: "draft" }),
  item("visa", "card"),
];

describe("searching items", () => {
  it("matches names, sites and tags, ignoring case", () => {
    const names = (text: string) => filterItems(items, { text, kind: null, drafts: false }).map((i) => i.name);
    expect(names("")).toHaveLength(4);
    expect(names("GIT")).toEqual(["github"]);
    expect(names("github.com")).toEqual(["github"]);
    expect(names("prod")).toEqual(["myapp/prod/db"]);
    expect(names("zzz")).toEqual([]);
  });

  it("narrows by kind and to drafts", () => {
    expect(filterItems(items, { text: "", kind: "login", drafts: false }).map((i) => i.name)).toEqual(["github", "vercel"]);
    expect(filterItems(items, { text: "", kind: null, drafts: true }).map((i) => i.name)).toEqual(["vercel"]);
  });

  it("shows a search box, a chip per kind present, and says when nothing matches", () => {
    const s = { ...initial, view: "items" as const, items };
    const html = render(s);
    expect(html).toContain('data-search');
    for (const word of ["모두", "로그인", "환경 변수", "카드", "draft만"]) expect(html).toContain(`>${word}</button>`);
    expect(html).not.toContain(">신원</button>");
    const none = render(search(s, { text: "zzz" }));
    expect(none).toContain("맞는 항목이 없습니다");
    expect(none).not.toContain('data-name="github"');
  });

  it("keeps the filter when moving between items but not between screens", () => {
    const s = search({ ...initial, view: "items" as const, items }, { kind: "card" });
    expect(s.filter.kind).toBe("card");
    expect(search(s, { text: "v" }).filter).toEqual({ text: "v", kind: "card", drafts: false });
  });
});
