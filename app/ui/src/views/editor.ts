// The item forms (09 §9.4): a new item of a kind, or a change to one. Secret fields are
// password inputs with nothing remembered; a change leaves empty fields as they are.

import type { Item, KindForm } from "../api.js";
import { esc } from "../html.js";
import { fieldWord, kindWord } from "../words.js";
import type { Editor } from "../state.js";

const quiet = 'autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false"';

const field = (name: string, secret: boolean, hint = "") =>
  `<label class="kh-field"><span class="kh-label">${esc(fieldWord(name))}</span>
    <input class="kh-input" name="v:${esc(name)}" type="${secret ? "password" : "text"}" placeholder="${esc(hint)}" ${quiet}></label>`;

/// How a kind's fields are grouped on screen (#123); fields not named here come last, alone.
const FIELD_GROUPS: Record<string, [title: string, fields: string[]][]> = {
  identity: [
    ["기본", ["full_name", "given_name", "family_name", "english_name", "hanja_name", "birth_date", "gender", "nationality"]],
    ["연락처", ["email", "phone", "emergency_phone"]],
    ["병역·보훈", ["military_service", "disability", "veterans"]],
  ],
};

/// `inputs` (field name → input markup) in the kind's groups, then the rest.
function grouped(kind: string, names: string[], input: (name: string) => string): string {
  const groups = FIELD_GROUPS[kind] ?? [];
  const placed = new Set(groups.flatMap(([, fields]) => fields));
  const sections = groups
    .map(([title, fields]) => {
      const shown = fields.filter((f) => names.includes(f)).map(input).join("");
      return shown ? `<fieldset class="field-group"><legend>${esc(title)}</legend>${shown}</fieldset>` : "";
    })
    .join("");
  return sections + names.filter((n) => !placed.has(n)).map(input).join("");
}

/// One empty row for a field the person names (`school` and its value).
export const customRow = (n: number) =>
  `<div class="custom-row"><input class="kh-input" name="cn:${n}" placeholder="이름 (예: school)" ${quiet}><input class="kh-input" name="cv:${n}" type="password" placeholder="값" ${quiet}></div>`;

/// Fields the person names, for kinds that take them (identity, address), after `existing`.
function customFields(existing = ""): string {
  return `<fieldset class="field-group custom"><legend>직접 추가한 항목</legend>${existing}<div class="custom-rows">${customRow(0)}</div>
    <button type="button" class="kh-btn ghost sm" data-action="add-custom-field">필드 추가</button></fieldset>`;
}

function newItem(kinds: KindForm[], kind: string): string {
  const spec = kinds.find((k) => k.kind === kind) ?? kinds[0];
  if (!spec) return "";
  const secret = new Map(spec.fields);
  const picker = kinds
    .map((k) => `<option value="${esc(k.kind)}"${k.kind === spec.kind ? " selected" : ""}>${esc(kindWord(k.kind))}</option>`)
    .join("");
  const inputs = spec.free_fields
    ? `<label class="kh-field"><span class="kh-label">KEY=VALUE (한 줄에 하나)</span><textarea class="kh-textarea" name="__env" rows="5" ${quiet}></textarea></label>`
    : grouped(spec.kind, spec.fields.map(([name]) => name), (name) => field(name, secret.get(name) ?? true)) +
      (spec.custom_fields ? customFields() : "");
  const generate = spec.kind === "login" ? `<label><input type="checkbox" name="generate"> 비밀번호 생성</label>` : "";
  return `<form class="kh-dialog" data-editor="new" data-kind="${esc(spec.kind)}"><div class="body">
    <h2>새 항목</h2>
    <label class="kh-field"><span class="kh-label">종류</span><select class="kh-input" name="kind" data-action="pick-kind">${picker}</select></label>
    <label class="kh-field"><span class="kh-label">이름</span><input class="kh-input" name="name" placeholder="github 또는 myapp/prod/db" ${quiet}></label>
    ${inputs}${generate}
    <label class="kh-field"><span class="kh-label">사이트 (한 줄에 하나)</span><textarea class="kh-textarea" name="origins" rows="2" ${quiet}></textarea></label>
    </div><div class="foot"><button type="button" class="kh-btn ghost" data-action="close-editor">취소</button>
    <button type="submit" class="kh-btn primary">저장</button></div></form>`;
}

function change(item: Item, spec: KindForm | undefined): string {
  const input = (f: string) => field(f, !(f in item.plain_values), "비우면 그대로");
  const declared = new Set((spec?.fields ?? []).map(([name]) => name));
  const own = spec?.custom_fields ? item.fields.filter((f) => !declared.has(f)) : [];
  const inputs = grouped(item.kind, item.fields.filter((f) => !own.includes(f)), input) +
    (spec?.custom_fields ? customFields(own.map(input).join("")) : "");
  return `<form class="kh-dialog" data-editor="edit" data-name="${esc(item.name)}"><div class="body">
    <h2>${esc(item.name)} 편집</h2>${inputs}
    <label class="kh-field"><span class="kh-label">사이트 (한 줄에 하나)</span>
      <textarea class="kh-textarea" name="origins" rows="2" ${quiet}>${item.origins.map(esc).join("\n")}</textarea></label>
    </div><div class="foot"><button type="button" class="kh-btn ghost" data-action="close-editor">취소</button>
    <button type="submit" class="kh-btn primary">저장</button></div></form>`;
}

/// A new name or folder for an item. Its placeholders change, so the form shows them first;
/// a policy rule that names the item makes the rename fail with that reason (J-104).
function rename(item: Item): string {
  const shown = item.placeholders.map((p) => `<code>${esc(p)}</code>`).join("<br>");
  return `<form class="kh-dialog" data-editor="rename" data-name="${esc(item.name)}"><div class="body">
    <h2>${esc(item.name)} 이름 바꾸기</h2>
    <label class="kh-field"><span class="kh-label">새 이름</span><input class="kh-input" name="to" value="${esc(item.name)}" placeholder="work/github" ${quiet}></label>
    <p class="kh-help error">이 placeholder가 새 이름으로 바뀝니다. 에이전트 설정에 옛 이름이 있으면 고쳐야 합니다.</p><p>${shown}</p>
    </div><div class="foot"><button type="button" class="kh-btn ghost" data-action="close-editor">취소</button>
    <button type="submit" class="kh-btn primary">이름 바꾸기</button></div></form>`;
}

export function editor(e: Editor, kinds: KindForm[], items: Item[]): string {
  if (e.mode === "new") return newItem(kinds, e.kind);
  const item = items.find((i) => i.name === e.name);
  if (!item) return "";
  return e.mode === "rename" ? rename(item) : change(item, kinds.find((k) => k.kind === item.kind));
}

/// Field values typed into an item form (`v:<field>` inputs, or KEY=VALUE lines), and the
/// sites, one per line.
export function editorValues(data: FormData): { values: Record<string, string>; origins: string[] } {
  const values: Record<string, string> = {};
  for (const [name, value] of data.entries()) {
    if (typeof value !== "string") continue;
    if (name.startsWith("v:")) values[name.slice(2)] = value;
    if (name.startsWith("cn:")) {
      const field = value.trim();
      const typed = data.get(`cv:${name.slice(3)}`);
      if (field && typeof typed === "string" && typed) values[field] = typed;
    }
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
