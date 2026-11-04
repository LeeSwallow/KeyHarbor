// The secure input form (09 §9.4.5): who asks, for what, why, where. Secret fields are
// password inputs; nothing is remembered, completed or spell-checked. The values stay in the
// inputs until "저장하고 보내기" and leave the page with the form.

import type { RequestForm } from "../api.js";
import { esc } from "../html.js";
import { fieldWord, kindWord } from "../words.js";

import { quiet } from "./parts.js";

/// One row for a field the person names in an answer (a credential's second password): always
/// secret here, since an agent asked for it (R-48).
const customRow = (n: number) =>
  `<div class="custom-row"><input class="kh-input" name="cn:${n}" placeholder="이름 (예: second_password)" ${quiet}><input class="kh-input" name="cv:${n}" type="password" placeholder="값" ${quiet}></div>`;

const CUSTOM_ROWS = 3;

function customFields(): string {
  const rows = Array.from({ length: CUSTOM_ROWS }, (_, n) => customRow(n)).join("");
  return `<fieldset class="field-group custom"><legend>직접 추가하는 필드</legend>${rows}</fieldset>`;
}

function input(name: string, secret: boolean): string {
  return `<label class="kh-field"><span class="kh-label">${esc(fieldWord(name))}</span>
    <input class="kh-input" name="${esc(name)}" type="${secret ? "password" : "text"}" ${quiet}></label>`;
}

export function form(f: RequestForm): string {
  const fields = f.free_fields
    ? `<label class="kh-field"><span class="kh-label">KEY=VALUE (한 줄에 하나)</span>
        <textarea class="kh-textarea" name="__env" rows="5" ${quiet}></textarea></label>`
    : f.fields.map((x) => input(x.name, x.secret)).join("") + (f.custom_fields ? customFields() : "");
  return `<form class="kh-dialog secure" data-form="${esc(f.id)}">
    <div class="body"><h2>${esc(f.agent)}이(가) ${esc(kindWord(f.kind))} 정보를 요청합니다</h2>
    <div class="who"><div>이유: ${esc(f.reason)}${f.origin ? `<br>사이트: ${esc(f.origin)}` : ""}<br>저장할 이름: <strong>${esc(f.name)}</strong></div></div>
    ${fields}
    <p class="kh-help">이 값은 에이전트에게 보이지 않습니다. 에이전트는 placeholder만 받습니다.</p></div>
    <div class="foot"><button type="button" class="kh-btn ghost" data-action="decline" data-id="${esc(f.id)}">거절</button>
    <button type="submit" class="kh-btn primary">저장하고 보내기</button></div>
  </form>`;
}

/// The values typed into a form: one per field, a row the person named, or KEY=VALUE lines for
/// env items. A named row that repeats a field is left out rather than replace it.
export function formValues(data: FormData): Record<string, string> {
  const values: Record<string, string> = {};
  for (const [name, value] of data.entries()) {
    if (typeof value !== "string" || name.startsWith("cv:")) continue;
    if (name.startsWith("cn:")) {
      const field = value.trim();
      const typed = data.get(`cv:${name.slice(3)}`);
      if (field && typeof typed === "string" && typed && !data.has(field) && !(field in values)) values[field] = typed;
    } else if (name === "__env") {
      for (const line of value.split("\n")) {
        const eq = line.indexOf("=");
        if (eq > 0) values[line.slice(0, eq).trim()] = line.slice(eq + 1);
      }
    } else {
      values[name] = value;
    }
  }
  return values;
}
