// State → HTML. Every text goes through `esc`; a value appears only for the field in
// `state.revealed`. Buttons carry `data-action` for the one click handler in main.ts.

import type { Item } from "./api.js";
import { SCREENS, type State, type View } from "./state.js";
import { dialog, policies } from "./views/admin.js";
import { settings } from "./views/settings.js";
import { editor } from "./views/editor.js";
import { form } from "./views/form.js";
import { palette, paletteCommands } from "./views/palette.js";

import { esc } from "./html.js";

export { esc };

const NAV = SCREENS;

function count(s: State, view: View): number {
  if (view === "inbox") return s.inbox.requests.length + s.inbox.drafts.length + s.handoffs.length;
  if (view === "sessions") return s.sessions.length;
  if (view === "items") return s.items.length;
  return 0;
}

function side(s: State): string {
  const links = NAV.map(([view, label]) => {
    const n = count(s, view);
    const badge = n > 0 ? `<span class="kh-count">${n}</span>` : "";
    return `<button class="nav${s.view === view ? " is-on" : ""}" data-action="go" data-view="${view}">${esc(label)}${badge}</button>`;
  }).join("");
  return `<nav class="side" aria-label="KeyHarbor">${links}<div class="group">인증 없음</div></nav>`;
}

function home(s: State): string {
  const o = s.overview;
  if (o && !o.vault_ready) {
    return `<div class="kh-empty"><p>금고가 없습니다. 터미널에서 <code>keyharbor vault init</code>을 실행하세요.</p></div>`;
  }
  const kv = (label: string, n: number | undefined) => `<span>${esc(label)}</span><strong>${n ?? "…"}</strong>`;
  const recent = s.audit
    .slice(0, 8)
    .map((e) => `<div class="kh-row">${esc(e.at)} · ${esc(e.agent)} · ${esc(e.kind)}${e.item ? ` · ${esc(e.item)}` : ""} · ${esc(e.result)}</div>`)
    .join("");
  return `<div class="kv">${kv("열린 세션", o?.sessions_open)}${kv("확인 대기", o ? o.requests_pending + o.drafts : undefined)}</div>
    <h2>최근 활동</h2>${recent || `<div class="kh-empty">기록이 없습니다</div>`}`;
}

function inbox(s: State): string {
  if (s.form) return form(s.form);
  const handoffs = s.handoffs.map(
    (h) => `<div class="kh-row"><div>인계 대기 · <strong>${esc(h.session_name)}</strong>${h.origin ? ` · ${esc(h.origin)}` : ""}</div>
      <button class="kh-btn" data-action="resume" data-session="${esc(h.session)}" data-id="${esc(h.request_id)}">재개</button></div>`,
  );
  const requests = s.inbox.requests.map(
    (r) => `<div class="kh-row"><div>${esc(r.agent)} → ${esc(r.kind)} <strong>${esc(r.name)}</strong>${r.origin ? ` · ${esc(r.origin)}` : ""}<br><small>${esc(r.reason)}</small></div>
      <button class="kh-btn" data-action="open-form" data-id="${esc(r.id)}">입력하기</button>
      <button class="kh-btn" data-action="open-window" data-id="${esc(r.id)}">창으로 열기</button>
      <button class="kh-btn" data-action="decline" data-id="${esc(r.id)}">거절</button></div>`,
  );
  const drafts = s.inbox.drafts.map(
    (d) => `<div class="kh-row"><div><strong>${esc(d.name)}</strong> · ${esc(d.kind)} · <span class="kh-badge">${esc(d.source)}</span></div>
      <button class="kh-btn" data-action="confirm" data-name="${esc(d.name)}">확정</button>
      <button class="kh-btn" data-action="discard" data-name="${esc(d.name)}">폐기</button></div>`,
  );
  const all = [...handoffs, ...requests, ...drafts].join("");
  return all || `<div class="kh-empty">확인할 것이 없습니다</div>`;
}

/// [열기]/[중지], or [복구…] for a session whose supervisor is gone (after a checkbox).
function sessionActions(id: string, state: string): string {
  const button = (action: string, label: string) =>
    `<button class="kh-btn" data-action="${action}" data-id="${esc(id)}">${label}</button>`;
  if (state === "recovery_required") {
    return `<label><input type="checkbox" data-closed="${esc(id)}"> 브라우저 창을 모두 닫았습니다</label>${button("recover", "복구…")}`;
  }
  return state === "stopped" ? button("open-session", "열기") : button("stop-session", "중지");
}

function sessions(s: State): string {
  return (
    s.sessions
      .map((x) => {
        const sites = x.sites.map((t) => `<div><small>${esc(t.origin)} · ${esc(t.state)}</small></div>`).join("");
        return `<div class="kh-row"><div><strong>${esc(x.name)}</strong> · ${esc(x.state)}${x.cdp ? ` · <span class="kh-badge">raw CDP</span>` : ""}${sites}</div>${sessionActions(x.id, x.state)}</div>`;
      })
      .join("") || `<div class="kh-empty">세션이 없습니다</div>`
  );
}

function itemList(s: State): string {
  const add = `<button class="kh-btn" data-action="new-item">+ 새 항목</button>`;
  return add + (
    s.items
      .map((i) => {
        const marks = [i.status === "draft" ? "draft" : "", i.tampered ? "변조 의심" : "", i.expired ? "만료" : ""]
          .filter(Boolean)
          .map((m) => `<span class="kh-badge">${esc(m)}</span>`)
          .join(" ");
        return `<button class="kh-row${s.selected === i.name ? " is-selected" : ""}" data-action="select" data-name="${esc(i.name)}"><strong>${esc(i.name)}</strong> ${esc(i.kind)} ${marks}</button>`;
      })
      .join("") || `<div class="kh-empty">항목이 없습니다</div>`
  );
}

function fieldRow(s: State, item: Item, field: string, placeholder: string): string {
  const plain = item.plain_values[field];
  const shown = s.revealed && s.revealed.item === item.name && s.revealed.field === field ? s.revealed.value : null;
  const value = plain ?? shown;
  const control =
    plain !== undefined || item.tampered
      ? ""
      : shown === null
        ? `<button class="kh-btn" data-action="reveal" data-name="${esc(item.name)}" data-field="${esc(field)}">보기</button>`
        : `<button class="kh-btn" data-action="hide">가리기</button>`;
  const copy = item.tampered || plain !== undefined ? "" : `<button class="kh-btn" data-action="copy" data-name="${esc(item.name)}" data-field="${esc(field)}">복사</button>`;
  return `<span>${esc(field)}</span><div><code>${value === null || value === undefined ? "••••••••" : esc(value)}</code> ${control} ${copy}<br><small>${esc(placeholder)}</small>
    <button class="kh-btn" data-action="copy-placeholder" data-text="${esc(placeholder)}" aria-label="placeholder 복사">⧉</button></div>`;
}

function detail(s: State): string {
  if (s.editor) return editor(s.editor, s.kinds, s.items);
  const item = s.items.find((i) => i.name === s.selected);
  if (!item) return `<div class="kh-empty">항목을 고르세요</div>`;
  const warn = item.tampered ? `<p class="kh-help">⚠ 변조 의심 — 이 항목은 입력에 쓰이지 않습니다</p>` : "";
  const fields = item.fields.map((f, n) => fieldRow(s, item, f, item.placeholders[n] ?? "")).join("");
  const origins = item.origins.map(esc).join(", ") || "—";
  return `<h2>${esc(item.name)}</h2>${warn}<div class="kv">${fields}<span>사이트</span><div>${origins}</div><span>태그</span><div>${item.tags.map(esc).join(", ") || "—"}</div></div>
    <button class="kh-btn" data-action="edit-item" data-name="${esc(item.name)}">편집</button>
    <button class="kh-btn" data-action="remove-item" data-name="${esc(item.name)}">삭제</button>`;
}

function audit(s: State): string {
  return s.audit
    .map((e) => `<div class="kh-row">${esc(e.at)} · ${esc(e.agent)} · ${esc(e.kind)}${e.item ? ` · ${esc(e.item)}` : ""}${e.origin ? ` · ${esc(e.origin)}` : ""} · ${esc(e.result)}</div>`)
    .join("") || `<div class="kh-empty">기록이 없습니다</div>`;
}

export function render(s: State): string {
  const error = s.error ? `<div class="kh-toast" role="alert">${esc(s.error)}</div>` : "";
  if (s.alone) {
    const body = s.form ? form(s.form) : `<div class="kh-empty">이 요청은 이미 끝났습니다</div>`;
    return `<section class="detail alone">${error}${body}</section>`;
  }
  const [list, main] =
    s.view === "items"
      ? [itemList(s), detail(s)]
      : ["", {
          home, inbox, sessions, audit,
          policies: (x: State) => policies(x.policies),
          settings: (x: State) => settings(x.settings),
        }[s.view](s)];
  return `${side(s)}<section class="list">${list}</section><section class="detail">${error}${main}</section>${dialog(s.ask)}${s.palette ? palette(s.palette, paletteCommands(s, s.palette.query)) : ""}`;
}
