// The settings screen (09 §9.4.7), built from the design system: switches (`kh-toggle`) for
// on/off, choices (`kh-seg`) for durations, words for every setting. What a setting means and
// which change weakens a protection stays in the domain registry; this table is only how the
// window shows it. A click saves at once; a weakening change comes back as a question.

import type { Setting } from "../api.js";
import { esc } from "../html.js";

type Control = { kind: "switch" } | { kind: "choices"; choices: [value: string, label: string][] };
interface Shown { group: string; label: string; help: string; control: Control }

const DAYS: [string, string][] = [["7d", "7일"], ["30d", "30일"], ["90d", "90일"], ["365d", "1년"]];

/// Settings in the order the screen shows them, by group.
const SHOWN: Record<string, Shown> = {
  audit_enabled: {
    group: "보안", label: "사용 기록 남기기", control: { kind: "switch" },
    help: "에이전트가 무엇을 썼는지 기록합니다. 값은 기록하지 않습니다. 끌 때는 확인합니다.",
  },
  capture_enabled: {
    group: "보안", label: "인계 중 로그인 포착", control: { kind: "switch" },
    help: "브라우저를 넘겨받아 직접 로그인할 때 입력한 계정을 draft로 저장합니다. 바꿀 때는 확인합니다.",
  },
  default_item_ttl: {
    group: "새 항목과 draft", label: "새 항목 만료", control: { kind: "choices", choices: [["none", "없음"], ...DAYS] },
    help: "새로 만든 항목이 이 기간 뒤 만료됩니다.",
  },
  draft_ttl: {
    group: "새 항목과 draft", label: "확정하지 않은 draft 지우기", control: { kind: "choices", choices: [["1d", "1일"], ...DAYS, ["none", "지우지 않음"]] },
    help: "에이전트가 만든 draft를 이 기간 뒤 정리합니다.",
  },
  clipboard_clear_seconds: {
    group: "클립보드", label: "복사한 값 자동 지우기", control: { kind: "choices", choices: [["10", "10초"], ["30", "30초"], ["60", "1분"], ["120", "2분"]] },
    help: "30초보다 길게 하면 확인합니다. 다른 것을 복사했으면 지우지 않습니다.",
  },
  notify_requests: { group: "알림", label: "정보 요청", control: { kind: "switch" }, help: "에이전트가 값을 입력해 달라고 할 때" },
  notify_handoffs: { group: "알림", label: "브라우저 인계", control: { kind: "switch" }, help: "에이전트가 로그인을 넘길 때" },
  notify_drafts: { group: "알림", label: "새 draft", control: { kind: "switch" }, help: "확정을 기다리는 항목이 생길 때" },
  notify_denials: { group: "알림", label: "거부", control: { kind: "switch" }, help: "정책이 에이전트의 사용을 막았을 때" },
};

const GROUPS = ["보안", "새 항목과 draft", "클립보드", "알림"];

const set = (key: string, value: string) => `data-action="set-setting" data-key="${esc(key)}" data-value="${esc(value)}"`;

function control(s: Setting, c: Control): string {
  if (c.kind === "switch") {
    const on = s.value === "true";
    return `<button type="button" class="kh-toggle${on ? " is-on" : ""}" role="switch" aria-checked="${on}" ${set(s.key, on ? "false" : "true")}><span class="track"></span></button>`;
  }
  // A value set elsewhere (the CLI takes any `<N>d`) stays visible as its own choice.
  const choices = c.choices.some(([v]) => v === s.value) ? c.choices : [...c.choices, [s.value, label(s.value)] as [string, string]];
  const buttons = choices
    .map(([v, text]) => `<button type="button"${v === s.value ? ' class="is-on"' : ""} ${set(s.key, v)}>${esc(text)}</button>`)
    .join("");
  return `<div class="kh-seg" role="group">${buttons}</div>`;
}

/// `45d` → `45일`, anything else as it is.
const label = (value: string) => (/^\d+d$/.test(value) ? `${value.slice(0, -1)}일` : value);

function row(s: Setting, shown: Shown): string {
  return `<div class="setting"><div><div class="title">${esc(shown.label)}</div><div class="kh-help">${esc(shown.help)}</div></div>${control(s, shown.control)}</div>`;
}

/// A setting this table does not know yet: a plain field, so it can still be changed.
function plain(s: Setting): string {
  return `<form class="setting" data-setting="${esc(s.key)}"><div><div class="title">${esc(s.key)}</div><div class="kh-help">기본 ${esc(s.default)}</div></div>
    <span><input class="kh-input" name="value" value="${esc(s.value)}" autocomplete="off" spellcheck="false"> <button class="kh-btn secondary" type="submit">저장</button></span></form>`;
}

export function settings(list: Setting[]): string {
  const sections = GROUPS.map((group) => {
    const rows = list.filter((s) => SHOWN[s.key]?.group === group).map((s) => row(s, SHOWN[s.key]!)).join("");
    return rows ? `<section class="settings-group"><h3>${group}</h3>${rows}</section>` : "";
  }).join("");
  const unknown = list.filter((s) => !SHOWN[s.key]).map(plain).join("");
  const others = unknown ? `<section class="settings-group"><h3>기타</h3>${unknown}</section>` : "";
  const auth = `<section class="settings-group"><h3>인증 방식</h3><div class="setting"><div><div class="title">앱과 값 보기 잠금</div>
    <div class="kh-help">인증 없음: 이 Mac 사용자 계정으로 접근할 수 있는 누구나 금고를 볼 수 있습니다.</div></div>
    <div class="kh-seg" role="group"><button type="button" class="is-on">없음</button><button type="button" disabled>macOS 인증 · 준비 중</button><button type="button" disabled>마스터 비밀번호 · 준비 중</button></div></div></section>`;
  return `<h2>설정</h2>${sections}${others}${auth}`;
}
