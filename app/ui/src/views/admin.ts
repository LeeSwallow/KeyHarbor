// Settings and policies (09 §9.4.4, §9.4.7), and the dialog that asks the person before a
// change that weakens a protection.

import type { Rule, Setting } from "../api.js";
import type { Ask } from "../state.js";
import { esc } from "../html.js";

export function settings(list: Setting[]): string {
  const rows = list
    .map(
      (s) => `<form class="kh-row" data-setting="${esc(s.key)}">
        <span class="main"><strong>${esc(s.key)}</strong> <small>기본 ${esc(s.default)}</small></span>
        <input class="kh-input" name="value" value="${esc(s.value)}" autocomplete="off" spellcheck="false">
        <button class="kh-btn" type="submit">저장</button></form>`,
    )
    .join("");
  return `<h2>설정</h2>${rows}<p class="kh-help">인증 방식: 없음 (macOS 인증 · 마스터 비밀번호 · 2FA는 준비 중)</p>`;
}

export function policies(rules: Rule[]): string {
  const rows = rules
    .map(
      (r) => `<div class="kh-row"><span class="main">${esc(r.agent)} · ${esc(r.resource)} ${esc(r.target)} → <strong>${esc(r.effect)}</strong></span>
        <button class="kh-btn" data-action="remove-rule" data-agent="${esc(r.agent)}" data-resource="${esc(r.resource)}" data-target="${esc(r.target)}">삭제</button></div>`,
    )
    .join("");
  const select = (name: string, options: string[]) =>
    `<select class="kh-input" name="${name}">${options.map((o) => `<option>${esc(o)}</option>`).join("")}</select>`;
  return `<h2>에이전트와 정책</h2>${rows || `<div class="kh-empty">규칙이 없습니다 (모두 허용)</div>`}
    <form class="kh-row" data-rule>
      <input class="kh-input" name="agent" placeholder="에이전트 (* 모두)" autocomplete="off" spellcheck="false">
      ${select("resource", ["item", "domain", "session", "create_session", "raw_cdp"])}
      <input class="kh-input" name="target" placeholder="대상 (* 모두)" autocomplete="off" spellcheck="false">
      ${select("effect", ["deny", "allow"])}
      <button class="kh-btn" type="submit">규칙 추가</button>
    </form>`;
}

export function dialog(ask: Ask | null): string {
  if (!ask) return "";
  return `<div class="kh-scrim"><div class="kh-dialog" role="alertdialog" aria-modal="true">
    <h2>${esc(ask.title ?? "보호를 약하게 하는 변경입니다")}</h2><p>${esc(ask.question)}</p>
    <button class="kh-btn" data-action="ask-cancel">취소</button>
    <button class="kh-btn" data-action="ask-confirm">확인</button></div></div>`;
}
