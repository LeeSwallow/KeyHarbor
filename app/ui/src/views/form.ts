// The secure input form (09 §9.4.5): who asks, for what, why, where. Secret fields are
// password inputs; nothing is remembered, completed or spell-checked. The values stay in the
// inputs until "저장하고 보내기" and leave the page with the form.

import type { RequestForm } from "../api.js";
import { esc } from "../html.js";

const quiet = 'autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false"';

function input(name: string, secret: boolean): string {
  return `<label class="kh-field"><span class="kh-label">${esc(name)}</span>
    <input class="kh-input" name="${esc(name)}" type="${secret ? "password" : "text"}" ${quiet}></label>`;
}

export function form(f: RequestForm): string {
  const fields = f.free_fields
    ? `<label class="kh-field"><span class="kh-label">KEY=VALUE (한 줄에 하나)</span>
        <textarea class="kh-textarea" name="__env" rows="5" ${quiet}></textarea></label>`
    : f.fields.map((x) => input(x.name, x.secret)).join("");
  return `<form class="kh-dialog" data-form="${esc(f.id)}">
    <h2>${esc(f.agent)} 가 ${esc(f.kind)} 정보를 요청합니다</h2>
    <p>이유: ${esc(f.reason)}${f.origin ? ` · 사이트: ${esc(f.origin)}` : ""}</p>
    <p>이름: <strong>${esc(f.name)}</strong></p>
    ${fields}
    <p class="kh-help">이 값은 에이전트에게 보이지 않습니다. 에이전트는 placeholder만 받습니다.</p>
    <button type="button" class="kh-btn" data-action="decline" data-id="${esc(f.id)}">거절</button>
    <button type="submit" class="kh-btn">저장하고 보내기</button>
  </form>`;
}

/// The values typed into a form: one per field, or KEY=VALUE lines for env items.
export function formValues(data: FormData): Record<string, string> {
  const values: Record<string, string> = {};
  for (const [name, value] of data.entries()) {
    if (typeof value !== "string") continue;
    if (name === "__env") {
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
