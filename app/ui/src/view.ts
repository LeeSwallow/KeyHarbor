// State → HTML. Every text goes through `esc`; a value appears only for the field in
// `state.revealed`. Buttons carry `data-action` for the one click handler in main.ts.

import type { AuditEvent, Item } from "./api.js";
import { SCREENS, type State, type View, filterItems } from "./state.js";
import { dialog, policies } from "./views/admin.js";
import { settings } from "./views/settings.js";
import { editor } from "./views/editor.js";
import { form } from "./views/form.js";
import { palette, paletteCommands } from "./views/palette.js";
import { badge, btn, card, empty, glyph, heading, icon, screenIcon } from "./views/parts.js";
import { auditWord, fieldWord, kindWord, sessionWord, siteWord, sourceWord } from "./words.js";

import { esc } from "./html.js";

export { esc };

const NAV = SCREENS;

function count(s: State, view: View): number {
  if (view === "inbox") return s.inbox.requests.length + s.inbox.drafts.length + s.handoffs.length;
  if (view === "sessions") return s.sessions.length;
  if (view === "items") return s.items.filter((i) => i.name !== s.removing).length;
  return 0;
}

function side(s: State): string {
  const links = NAV.map(([view, label]) => {
    const n = count(s, view);
    const badge = n > 0 ? `<span class="kh-count">${n}</span>` : "";
    return `<button class="nav${s.view === view ? " is-on" : ""}" data-action="go" data-view="${view}">${icon(screenIcon(view))}${esc(label)}${badge}</button>`;
  }).join("");
  return `<nav class="side" aria-label="KeyHarbor"><div class="brand"><img class="brand-icon" src="app-icon.svg" alt="">KeyHarbor</div>${links}<div class="group">인증 없음</div></nav>`;
}

/// `2026-10-29T05:02:00Z` → `10-29 05:02` (UTC as stored; the record is a glance, not a log).
const when = (at: string) => at.replace(/^\d{4}-(\d\d-\d\d)T(\d\d:\d\d).*$/, "$1 $2");

function event(e: AuditEvent): string {
  const denied = e.kind.includes("denied");
  const what = [e.item, e.origin].filter(Boolean).map((t) => esc(t!)).join(" · ");
  return card({
    title: `${esc(auditWord(e.kind))} ${denied ? badge("거부", "denied") : ""}`,
    sub: `${esc(e.agent)}${what ? ` · ${what}` : ""}`,
    actions: `<span class="meta">${esc(when(e.at))}</span>`,
  });
}

function home(s: State): string {
  const o = s.overview;
  if (o && !o.vault_ready) {
    return `${heading("home")}<div class="kh-empty"><p>금고가 없습니다. 터미널에서 <code>keyharbor vault init</code>을 실행하세요.</p></div>`;
  }
  const stat = (label: string, n: number | undefined, view: View) =>
    `<button type="button" class="stat" data-action="go" data-view="${view}"><span class="kh-help">${esc(label)}</span><strong>${n ?? "…"}</strong></button>`;
  const recent = s.audit.slice(0, 6).map(event).join("");
  return `${heading("home")}<div class="stats">${stat("실행 중인 세션", o?.sessions_open, "sessions")}${stat("확인 대기", o ? count(s, "inbox") : undefined, "inbox")}${stat("항목", s.items.length, "items")}</div>
    <h3 class="section">최근 활동</h3>${recent || empty("기록이 없습니다")}`;
}

function inbox(s: State): string {
  if (s.form) return form(s.form);
  const handoffs = s.handoffs.map((h) =>
    card({
      glyph: `<span class="glyph session">${icon("hand")}</span>`,
      title: `${esc(h.session_name)} 세션이 로그인을 넘겼습니다`,
      sub: h.origin ? `${esc(h.origin)}에서 마친 뒤 재개하세요` : "브라우저에서 마친 뒤 재개하세요",
      actions: btn("재개", `data-action="resume" data-session="${esc(h.session)}" data-id="${esc(h.request_id)}"`, "primary"),
    }),
  );
  const requests = s.inbox.requests.map((r) =>
    card({
      glyph: glyph(r.kind),
      title: `${esc(r.agent)}이(가) ${esc(kindWord(r.kind))} <strong>${esc(r.name)}</strong>을(를) 요청합니다`,
      sub: `${esc(r.reason)}${r.origin ? ` · ${esc(r.origin)}` : ""}`,
      actions:
        btn("거절", `data-action="decline" data-id="${esc(r.id)}"`, "ghost") +
        btn("창으로 열기", `data-action="open-window" data-id="${esc(r.id)}"`) +
        btn("입력하기", `data-action="open-form" data-id="${esc(r.id)}"`, "primary"),
    }),
  );
  const drafts = s.inbox.drafts.map((d) =>
    card({
      glyph: glyph(d.kind),
      title: `${esc(d.name)} ${badge("draft", "draft")}`,
      sub: `${esc(kindWord(d.kind))} · ${esc(sourceWord(d.source))}${d.origins.length ? ` · ${d.origins.map(esc).join(", ")}` : ""}`,
      actions:
        btn("폐기", `data-action="discard" data-name="${esc(d.name)}"`, "ghost") +
        btn("확정", `data-action="confirm" data-name="${esc(d.name)}"`, "primary"),
    }),
  );
  const all = [...handoffs, ...requests, ...drafts].join("");
  return `${heading("inbox", "에이전트가 사람을 기다리는 것들입니다. 에이전트는 값을 보지 못합니다.")}${all || empty("확인할 것이 없습니다")}`;
}

/// [열기]/[중지], or [복구…] for a session whose supervisor is gone (after a checkbox).
function sessionActions(id: string, state: string): string {
  if (state === "recovery_required") {
    return `<label class="kh-help"><input type="checkbox" data-closed="${esc(id)}"> 브라우저 창을 모두 닫았습니다</label>${btn("복구…", `data-action="recover" data-id="${esc(id)}"`, "danger")}`;
  }
  return state === "stopped"
    ? btn("열기", `data-action="open-session" data-id="${esc(id)}"`, "primary")
    : btn("중지", `data-action="stop-session" data-id="${esc(id)}"`);
}

function sessionBadge(state: string): string {
  const tone = state === "running" ? "active" : state === "recovery_required" || state === "failed" ? "denied" : "neutral";
  return badge(sessionWord(state), tone);
}

function sessions(s: State): string {
  const rows = s.sessions
    .map((x) => {
      const sites = x.sites
        .map((t) => `<li>${esc(t.origin)} ${badge(siteWord(t.state), t.state === "logged_in" ? "active" : t.state === "expired" ? "expiring" : "neutral")}</li>`)
        .join("");
      return card({
        glyph: `<span class="glyph session">${icon("app-window")}</span>`,
        title: `${esc(x.name)} ${sessionBadge(x.state)}${x.cdp ? ` ${badge("raw CDP", "expiring")}` : ""}`,
        sub: x.description ? esc(x.description) : "",
        extra: sites ? `<ul class="sites">${sites}</ul>` : "",
        actions: sessionActions(x.id, x.state),
      });
    })
    .join("");
  return `${heading("sessions", "에이전트가 쓰는 전용 브라우저입니다. 사이트별 로그인 상태가 함께 보입니다.")}${rows || empty("세션이 없습니다")}`;
}

/// The search box, a chip per kind the vault has, and "draft만" (09 §9.4 검색 · 필터).
function searchBar(s: State): string {
  const f = s.filter;
  const chip = (label: string, on: boolean, attrs: string) =>
    `<button type="button" class="kh-chip${on ? " is-on" : ""}" ${attrs}>${esc(label)}</button>`;
  const kinds = [...new Set(s.items.map((i) => i.kind))];
  const chips = [
    chip("모두", !f.kind, `data-action="filter-kind" data-kind=""`),
    ...kinds.map((k) => chip(kindWord(k), f.kind === k, `data-action="filter-kind" data-kind="${esc(k)}"`)),
    chip("draft만", f.drafts, `data-action="filter-drafts"`),
  ].join("");
  return `<div class="search"><span class="kh-inputwrap">${icon("search")}<input class="kh-input" data-search placeholder="이름, 사이트, 태그 (⌘F)" value="${esc(f.text)}" autocomplete="off" spellcheck="false"></span><div class="chips">${chips}</div></div>`;
}

function itemList(s: State): string {
  const add = `<div class="list-head">${heading("items")}${btn("새 항목", `data-action="new-item"`, "primary")}</div>`;
  const rows = filterItems(s.items.filter((i) => i.name !== s.removing), s.filter)
    .map((i) => {
      const marks = [
        i.status === "draft" ? badge("draft", "draft") : "",
        i.tampered ? badge("변조 의심", "tampered") : "",
        i.expired ? badge("만료", "expired") : "",
      ].join(" ");
      return `<button type="button" class="kh-row${s.selected === i.name ? " is-selected" : ""}" data-action="select" data-name="${esc(i.name)}">${glyph(i.kind)}
        <span class="main"><span class="title">${esc(i.name)}</span><span class="sub">${esc(kindWord(i.kind))}${i.origins[0] ? ` · ${esc(i.origins[0])}` : ""}</span></span>${marks}</button>`;
    })
    .join("");
  const none = s.items.length ? empty("맞는 항목이 없습니다") : empty("항목이 없습니다");
  return add + (s.items.length ? searchBar(s) : "") + (rows || none);
}

function fieldRow(s: State, item: Item, field: string, placeholder: string): string {
  const plain = item.plain_values[field];
  const shown = s.revealed && s.revealed.item === item.name && s.revealed.field === field ? s.revealed.value : null;
  const value = plain ?? shown;
  const at = `data-name="${esc(item.name)}" data-field="${esc(field)}"`;
  const control =
    plain !== undefined || item.tampered
      ? ""
      : shown === null
        ? btn("보기", `data-action="reveal" ${at}`, "ghost")
        : btn("가리기", `data-action="hide"`, "ghost");
  const copy = item.tampered || plain !== undefined ? "" : btn("복사", `data-action="copy" ${at}`, "ghost");
  return `<span>${esc(fieldWord(field))}</span><div class="field-value"><div class="value-line"><code>${value === null || value === undefined ? "••••••••" : esc(value)}</code>${control}${copy}</div>
    <div class="value-line"><small class="kh-help">${esc(placeholder)}</small><button type="button" class="kh-btn ghost sm" data-action="copy-placeholder" data-text="${esc(placeholder)}" aria-label="placeholder 복사">${icon("copy")}</button></div></div>`;
}

function detail(s: State): string {
  if (s.editor) return editor(s.editor, s.kinds, s.items);
  const item = s.items.find((i) => i.name === s.selected);
  if (!item) return empty("항목을 고르세요");
  const warn = item.tampered ? `<p class="kh-help error">${icon("triangle-alert")} 변조 의심 — 이 항목은 입력에 쓰이지 않습니다</p>` : "";
  const fields = item.fields.map((f, n) => fieldRow(s, item, f, item.placeholders[n] ?? "")).join("");
  const origins = item.origins.map(esc).join("<br>") || "—";
  const actions =
    btn("편집", `data-action="edit-item" data-name="${esc(item.name)}"`) +
    btn("이름 바꾸기", `data-action="rename-item" data-name="${esc(item.name)}"`, "ghost") +
    btn("삭제", `data-action="remove-item" data-name="${esc(item.name)}"`, "danger");
  return `<header class="item-head">${glyph(item.kind)}<div class="main"><h2>${esc(item.name)}</h2><div class="kh-help">${esc(kindWord(item.kind))} · ${esc(sourceWord(item.source))}</div></div><div class="actions">${actions}</div></header>
    ${warn}${item.description ? `<p>${esc(item.description)}</p>` : ""}
    <div class="kv">${fields}<span>사이트</span><div>${origins}</div><span>태그</span><div>${item.tags.map(esc).join(", ") || "—"}</div></div>`;
}

function audit(s: State): string {
  return `${heading("audit", "최근 50개. 값은 기록하지 않습니다.")}${s.audit.map(event).join("") || empty("기록이 없습니다")}`;
}

export function render(s: State): string {
  const error = s.error ? `<div class="kh-toast" role="alert">${esc(s.error)}</div>` : "";
  const undo = s.removing
    ? `<div class="kh-toast undo" role="status"><span class="msg">${esc(s.removing)}을(를) 지웠습니다</span><button type="button" class="act" data-action="undo-remove">실행 취소</button></div>`
    : "";
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
  return `${side(s)}<section class="list">${list}</section><section class="detail">${error}${main}</section>${undo}${dialog(s.ask)}${s.palette ? palette(s.palette, paletteCommands(s, s.palette.query)) : ""}`;
}
