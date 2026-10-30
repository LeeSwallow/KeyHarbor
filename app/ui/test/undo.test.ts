import { describe, expect, it } from "vitest";
import type { Item } from "../src/api.js";
import { initial, removing, select, undoRemove } from "../src/state.js";
import { render } from "../src/view.js";

const item = (name: string): Item => ({
  name, kind: "login", domain: "credentials", status: "active", source: "user", description: null,
  fields: [], placeholders: [], plain_values: {}, origins: [], tags: [], expired: false, tampered: false,
});
const s = select({ ...initial, view: "items" as const, items: [item("github"), item("bank")] }, "github");

describe("removing an item", () => {
  it("hides it at once and offers to undo", () => {
    const gone = removing(s, "github");
    expect(gone.selected).toBeNull();
    const html = render(gone);
    expect(html).not.toContain('data-action="select" data-name="github"');
    expect(html).toContain('data-name="bank"');
    expect(html).toContain("github을(를) 지웠습니다");
    expect(html).toContain('data-action="undo-remove"');
    expect(html).toMatch(/항목<span class="kh-count">1<\/span>/);
  });

  it("brings it back on undo", () => {
    const back = undoRemove(removing(s, "github"));
    expect(back.removing).toBeNull();
    expect(render(back)).toContain('data-action="select" data-name="github"');
  });

  it("asks no question first: undo replaces the confirmation", () => {
    expect(render(removing(s, "github"))).not.toContain("alertdialog");
  });
});
