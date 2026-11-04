// The item forms (09 §9.4): a new item of a kind, or a change to one. Secret fields are
// password inputs with nothing remembered; a change leaves empty fields as they are. Every kind
// but env takes fields the person names, each secret unless marked 일반 (R-48); a new
// credential can start from a template's fields.

import type { Item, KindForm, Template, TemplateField } from "../api.js";
import { esc } from "../html.js";
import { fieldWord, kindWord } from "../words.js";
import type { Editor } from "../state.js";
import { quiet } from "./parts.js";

const field = (name: string, secret: boolean, hint = "") =>
  `<label class="kh-field"><span class="kh-label">${esc(fieldWord(name))}</span>
    <input class="kh-input" name="v:${esc(name)}" type="${secret ? "password" : "text"}" placeholder="${esc(hint)}" ${quiet}></label>`;

/// What a field takes, shown in its empty input (a seed comes as base32 or a URI, #206).
const HINTS: Record<string, string> = { totp_seed: "base32 또는 otpauth:// URI" };

/// How a kind's fields are grouped on screen (#123); fields not named here come last, alone.
const FIELD_GROUPS: Record<string, [title: string, fields: string[]][]> = {
  identity: [
    ["기본", ["full_name", "given_name", "family_name", "english_name", "hanja_name", "birth_date", "gender", "nationality"]],
    ["연락처", ["email", "phone", "emergency_phone"]],
    ["병역·보훈", ["military_service", "disability", "veterans"]],
  ],
};

/// The kind a template fills (R-48).
export const TEMPLATE_KIND = "credential";

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

/// One row for a field the person names (`school` and its value), secret unless 일반 is
/// ticked. `from` fills the name and the choice from a template field.
export const customRow = (n: number, from?: TemplateField) => {
  const plain = from ? !from.secret : false;
  const named = from ? ` value="${esc(from.name)}"` : "";
  return `<div class="custom-row"><input class="kh-input" name="cn:${n}" placeholder="이름 (예: school)"${named} ${quiet}><input class="kh-input" name="cv:${n}" type="${plain ? "text" : "password"}" placeholder="값" ${quiet}>
    <label class="kh-help"><input type="checkbox" name="cp:${n}"${plain ? " checked" : ""}> 일반</label></div>`;
};

/// Fields the person names, after `existing`; `rows` start from a template's fields.
function customFields(existing = "", rows: TemplateField[] = []): string {
  const fresh = rows.length ? rows.map((f, i) => customRow(i, f)).join("") : customRow(0);
  return `<fieldset class="field-group custom"><legend>직접 추가한 항목</legend>${existing}<div class="custom-rows">${fresh}</div>
    <button type="button" class="kh-btn ghost sm" data-action="add-custom-field">필드 추가</button>
    <p class="kh-help">일반으로 둔 필드는 목록에 값이 보이고 가려지지 않습니다.</p></fieldset>`;
}

/// The template picker for a new credential, with `picked` selected.
function templatePicker(templates: Template[], picked: string | undefined): string {
  if (!templates.length) return "";
  const options = templates
    .map((t) => `<option value="${esc(t.name)}"${t.name === picked ? " selected" : ""}>${esc(t.name)}</option>`)
    .join("");
  return `<label class="kh-field"><span class="kh-label">자격 증명 틀</span><select class="kh-input" name="template" data-action="pick-template">
    <option value="">틀 없이</option>${options}</select></label>`;
}

function newItem(kinds: KindForm[], e: Extract<Editor, { mode: "new" }>, templates: Template[]): string {
  const spec = kinds.find((k) => k.kind === e.kind) ?? kinds[0];
  if (!spec) return "";
  const secret = new Map(spec.fields.map((f) => [f.name, f.secret]));
  const picker = kinds
    .map((k) => `<option value="${esc(k.kind)}"${k.kind === spec.kind ? " selected" : ""}>${esc(kindWord(k.kind))}</option>`)
    .join("");
  const fromTemplate = spec.kind === TEMPLATE_KIND;
  const rows = fromTemplate ? (templates.find((t) => t.name === e.template)?.fields ?? []) : [];
  const inputs = spec.free_fields
    ? `<label class="kh-field"><span class="kh-label">KEY=VALUE (한 줄에 하나)</span><textarea class="kh-textarea" name="__env" rows="5" ${quiet}></textarea></label>`
    : (fromTemplate ? templatePicker(templates, e.template) : "") +
      grouped(spec.kind, spec.fields.map((f) => f.name), (name) => field(name, secret.get(name) ?? true, HINTS[name])) +
      (spec.custom_fields ? customFields("", rows) : "");
  const generate = spec.generates ? `<label><input type="checkbox" name="generate"> ${esc(fieldWord(spec.generates))} 생성</label>` : "";
  return `<form class="kh-dialog" data-editor="new" data-kind="${esc(spec.kind)}"><div class="body">
    <h2>새 항목</h2>
    <label class="kh-field"><span class="kh-label">종류</span><select class="kh-input" name="kind" data-action="pick-kind">${picker}</select></label>
    <label class="kh-field"><span class="kh-label">이름</span><input class="kh-input" name="name" placeholder="github 또는 myapp/prod/db" ${quiet}></label>
    ${inputs}${generate}
    <label class="kh-field"><span class="kh-label">사이트 (한 줄에 하나)</span><textarea class="kh-textarea" name="origins" rows="2" ${quiet}></textarea></label>
    </div><div class="foot"><button type="button" class="kh-btn ghost" data-action="close-editor">취소</button>
    <button type="submit" class="kh-btn primary">저장</button></div></form>`;
}

/// A field the item has that the person named: its value, whether it is plain (asks first to
/// make plain, R-35), and taking it out. `pw:` remembers that it was plain.
function ownField(item: Item, name: string): string {
  const plain = name in item.plain_values;
  return `<div class="own-field">${field(name, !plain, "비우면 그대로")}
    <label class="kh-help"><input type="checkbox" name="pl:${esc(name)}"${plain ? " checked" : ""}> 일반</label>
    ${plain ? `<input type="hidden" name="pw:${esc(name)}" value="1">` : ""}
    <label class="kh-help"><input type="checkbox" name="rm:${esc(name)}"> 제거</label></div>`;
}

function change(item: Item, spec: KindForm | undefined): string {
  const input = (f: string) => field(f, !(f in item.plain_values), HINTS[f] ? `${HINTS[f]}, 비우면 그대로` : "비우면 그대로");
  const declared = new Set((spec?.fields ?? []).map((f) => f.name));
  const own = spec?.custom_fields ? item.fields.filter((f) => !declared.has(f)) : [];
  const inputs = grouped(item.kind, item.fields.filter((f) => !own.includes(f)), input) +
    (spec?.custom_fields ? customFields(own.map((f) => ownField(item, f)).join("")) : "");
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

export function editor(e: Editor, kinds: KindForm[], items: Item[], templates: Template[] = []): string {
  if (e.mode === "new") return newItem(kinds, e, templates);
  const item = items.find((i) => i.name === e.name);
  if (!item) return "";
  return e.mode === "rename" ? rename(item) : change(item, kinds.find((k) => k.kind === item.kind));
}

/// What an item form sends: field values (`v:<field>` inputs, rows the person named, or
/// KEY=VALUE lines), the sites one per line, which fields to mark plain or secret again or
/// take out, and `clashes`: named rows that repeat a field the form already has (refused).
export interface EditorValues {
  values: Record<string, string>;
  origins: string[];
  plain: string[];
  secret: string[];
  remove: string[];
  clashes: string[];
}

export function editorValues(data: FormData): EditorValues {
  const out: EditorValues = { values: {}, origins: [], plain: [], secret: [], remove: [], clashes: [] };
  const named = new Set<string>();
  for (const [name, value] of data.entries()) {
    if (typeof value !== "string") continue;
    if (name.startsWith("v:")) out.values[name.slice(2)] = value;
    if (name.startsWith("cn:")) customValue(data, name.slice(3), value.trim(), named, out);
    if (name.startsWith("rm:")) out.remove.push(name.slice(3));
    if (name === "__env") {
      for (const line of value.split("\n")) {
        const eq = line.indexOf("=");
        if (eq > 0) out.values[line.slice(0, eq).trim()] = line.slice(eq + 1);
      }
    }
  }
  ownMarks(data, out);
  out.origins = String(data.get("origins") ?? "").split("\n").map((o) => o.trim()).filter(Boolean);
  return out;
}

/// A row the person named (`cn:<n>`, `cv:<n>`, `cp:<n>`): kept when it has a name and a value.
function customValue(data: FormData, n: string, field: string, named: Set<string>, out: EditorValues): void {
  const typed = data.get(`cv:${n}`);
  if (!field || typeof typed !== "string" || !typed) return;
  if (data.has(`v:${field}`) || named.has(field)) {
    out.clashes.push(field);
    return;
  }
  named.add(field);
  out.values[field] = typed;
  if (data.get(`cp:${n}`) === "on") out.plain.push(field);
}

/// The plain box of each field the item has (only fields the person named show one): ticked
/// now and not before marks it plain, unticked now and plain before (`pw:`) marks it secret
/// again. A field taken out is neither.
function ownMarks(data: FormData, out: EditorValues): void {
  for (const key of data.keys()) {
    const name = key.slice(2);
    if (!key.startsWith("v:") || out.remove.includes(name)) continue;
    const now = data.get(`pl:${name}`) === "on";
    const before = data.has(`pw:${name}`);
    if (now && !before) out.plain.push(name);
    if (!now && before) out.secret.push(name);
  }
}
