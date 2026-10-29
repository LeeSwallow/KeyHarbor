// The item forms (09 §9.4): a new item of a kind, or a change to one. Secret fields are
// password inputs with nothing remembered; a change leaves empty fields as they are.

import type { Item, KindForm } from "../api.js";
import { esc } from "../html.js";
import type { Editor } from "../state.js";

const quiet = 'autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false"';

const field = (name: string, secret: boolean, hint = "") =>
  `<label class="kh-field"><span class="kh-label">${esc(name)}</span>
    <input class="kh-input" name="v:${esc(name)}" type="${secret ? "password" : "text"}" placeholder="${esc(hint)}" ${quiet}></label>`;

function newItem(kinds: KindForm[], kind: string): string {
  const spec = kinds.find((k) => k.kind === kind) ?? kinds[0];
  if (!spec) return "";
  const picker = kinds
    .map((k) => `<option value="${esc(k.kind)}"${k.kind === spec.kind ? " selected" : ""}>${esc(k.kind)}</option>`)
    .join("");
  const inputs = spec.free_fields
    ? `<label class="kh-field"><span class="kh-label">KEY=VALUE (한 줄에 하나)</span><textarea class="kh-textarea" name="__env" rows="5" ${quiet}></textarea></label>`
    : spec.fields.map(([name, secret]) => field(name, secret)).join("");
  const generate = spec.kind === "login" ? `<label><input type="checkbox" name="generate"> 비밀번호 생성</label>` : "";
  return `<form class="kh-dialog" data-editor="new" data-kind="${esc(spec.kind)}">
    <h2>새 항목</h2>
    <label class="kh-field"><span class="kh-label">종류</span><select class="kh-input" name="kind" data-action="pick-kind">${picker}</select></label>
    <label class="kh-field"><span class="kh-label">이름</span><input class="kh-input" name="name" placeholder="github 또는 myapp/prod/db" ${quiet}></label>
    ${inputs}${generate}
    <label class="kh-field"><span class="kh-label">사이트 (한 줄에 하나)</span><textarea class="kh-textarea" name="origins" rows="2" ${quiet}></textarea></label>
    <button type="button" class="kh-btn" data-action="close-editor">취소</button>
    <button type="submit" class="kh-btn">저장</button></form>`;
}

function change(item: Item): string {
  const inputs = item.fields.map((f) => field(f, !(f in item.plain_values), "비우면 그대로")).join("");
  return `<form class="kh-dialog" data-editor="edit" data-name="${esc(item.name)}">
    <h2>${esc(item.name)} 편집</h2>${inputs}
    <label class="kh-field"><span class="kh-label">사이트 (한 줄에 하나)</span>
      <textarea class="kh-textarea" name="origins" rows="2" ${quiet}>${item.origins.map(esc).join("\n")}</textarea></label>
    <button type="button" class="kh-btn" data-action="close-editor">취소</button>
    <button type="submit" class="kh-btn">저장</button></form>`;
}

export function editor(e: Editor, kinds: KindForm[], items: Item[]): string {
  if (e.mode === "new") return newItem(kinds, e.kind);
  const item = items.find((i) => i.name === e.name);
  return item ? change(item) : "";
}

/// Field values typed into an item form (`v:<field>` inputs, or KEY=VALUE lines), and the
/// sites, one per line.
export function editorValues(data: FormData): { values: Record<string, string>; origins: string[] } {
  const values: Record<string, string> = {};
  for (const [name, value] of data.entries()) {
    if (typeof value !== "string") continue;
    if (name.startsWith("v:")) values[name.slice(2)] = value;
    if (name === "__env") {
      for (const line of value.split("\n")) {
        const eq = line.indexOf("=");
        if (eq > 0) values[line.slice(0, eq).trim()] = line.slice(eq + 1);
      }
    }
  }
  const origins = String(data.get("origins") ?? "").split("\n").map((o) => o.trim()).filter(Boolean);
  return { values, origins };
}
