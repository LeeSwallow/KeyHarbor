// Policies (09 §9.4.4), and the dialog that asks the person before a
// change that weakens a protection.

import type { Rule } from "../api.js";
import type { Ask } from "../state.js";
import { esc } from "../html.js";
import { domainWord, effectWord, resourceWord } from "../words.js";
import { badge, btn, card, empty, heading } from "./parts.js";

const RESOURCES = ["item", "domain", "session", "create_session", "raw_cdp"];

/// `* → 모든 …`, a domain in words, anything else as typed.
const target = (r: Rule) => (r.target === "*" ? "모두" : r.resource === "domain" ? domainWord(r.target) : r.target);

export function policies(rules: Rule[]): string {
  const rows = rules
    .map((r) =>
      card({
        title: `${esc(r.agent === "*" ? "모든 에이전트" : r.agent)} · ${esc(resourceWord(r.resource))} ${esc(target(r))} ${badge(effectWord(r.effect), r.effect === "deny" ? "denied" : "active")}`,
        actions: btn("삭제", `data-action="remove-rule" data-agent="${esc(r.agent)}" data-resource="${esc(r.resource)}" data-target="${esc(r.target)}"`, "ghost"),
      }),
    )
    .join("");
  const options = (values: string[], word: (v: string) => string) =>
    values.map((v) => `<option value="${esc(v)}">${esc(word(v))}</option>`).join("");
  return `${heading("에이전트와 정책", "규칙이 없으면 모두 허용합니다. 허용을 더하거나 거부를 지울 때는 확인합니다.")}
    ${rows || empty("규칙이 없습니다 (모두 허용)")}
    <h3 class="section">규칙 추가</h3>
    <form class="rule-form" data-rule>
      <label class="kh-field"><span class="kh-label">에이전트</span><input class="kh-input" name="agent" placeholder="* 모두" autocomplete="off" spellcheck="false"></label>
      <label class="kh-field"><span class="kh-label">대상 종류</span><select class="kh-input" name="resource">${options(RESOURCES, resourceWord)}</select></label>
      <label class="kh-field"><span class="kh-label">대상</span><input class="kh-input" name="target" placeholder="* 모두 · 항목 이름 · payment" autocomplete="off" spellcheck="false"></label>
      <label class="kh-field"><span class="kh-label">효과</span><select class="kh-input" name="effect">${options(["deny", "allow"], effectWord)}</select></label>
      <button class="kh-btn primary" type="submit">규칙 추가</button>
    </form>`;
}

export function dialog(ask: Ask | null): string {
  if (!ask) return "";
  return `<div class="kh-scrim"><div class="kh-dialog" role="alertdialog" aria-modal="true"><div class="body">
    <h2>${esc(ask.title ?? "보호를 약하게 하는 변경입니다")}</h2><p>${esc(ask.question)}</p></div>
    <div class="foot">${btn("취소", `data-action="ask-cancel"`, "ghost", "")}${btn("확인", `data-action="ask-confirm"`, "danger", "")}</div></div></div>`;
}
