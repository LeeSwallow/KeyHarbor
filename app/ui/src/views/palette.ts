// The command palette (⌘K, 09 U6): every screen, a new item, and every item by name, narrowed
// by what was typed. Commands are data; main.ts runs the one picked.

import { esc } from "../html.js";
import { type Palette, SCREENS, type State, type View } from "../state.js";
import { icon } from "./parts.js";

export type Run = { kind: "go"; view: View } | { kind: "select"; name: string } | { kind: "new-item" };
export interface Command { label: string; run: Run; icon?: string }

/// How many matches the palette shows.
const SHOWN = 8;

const plain = (text: string) => text.toLowerCase().replace(/\s+/g, "");

export function paletteCommands(s: State, typed: string): Command[] {
  const all: Command[] = [
    ...SCREENS.map(([view, label, icon]): Command => ({ label: `${label} 화면으로 가기`, run: { kind: "go", view }, icon })),
    { label: "새 항목", run: { kind: "new-item" }, icon: "plus" },
    ...s.items.map((i): Command => ({ label: `${i.name} 열기`, run: { kind: "select", name: i.name } })),
  ];
  const want = plain(typed);
  return all.filter((c) => plain(c.label).includes(want));
}

export function palette(p: Palette, matches: Command[]): string {
  const rows = matches
    .slice(0, SHOWN)
    .map((c, n) => `<button class="kh-row" data-action="run" data-index="${n}">${c.icon ? icon(c.icon) : ""}${esc(c.label)}</button>`)
    .join("");
  return `<div class="kh-scrim"><div class="kh-dialog" role="dialog" aria-modal="true" aria-label="명령">
    <input class="kh-input" data-palette placeholder="화면, 항목, 명령" value="${esc(p.query)}" autocomplete="off" spellcheck="false">
    ${rows || `<div class="kh-empty">맞는 것이 없습니다</div>`}</div></div>`;
}
