// The settings screen (09 §9.4.7), built from the design system: switches (`kh-toggle`) for
// on/off, choices (`kh-seg`) for durations, words for every setting. What a setting means and
// which change weakens a protection stays in the domain registry; this table is only how the
// window shows it. A click saves at once; a weakening change comes back as a question.

import type { Browsers, Setting, Weakening } from "../api.js";
import { esc } from "../html.js";
import { NO_AUTH } from "../words.js";
import { heading } from "./parts.js";

type Control =
  | { kind: "switch" }
  | { kind: "choices"; choices: [value: string, label: string][] }
  | { kind: "folders" };
/// `above` words the rule "more than the default asks first" with the number Rust sends
/// (`Setting.weakening`), so the sentence cannot drift from the rule (J-203).
interface Shown { group: string; label: string; help: string; control: Control; above?: (than: string) => string }

const DAYS: [string, string][] = [["7d", "7일"], ["30d", "30일"], ["90d", "90일"], ["365d", "1년"]];

/// Choices counted in seconds: `60` → `1분`, `15` → `15초`.
function seconds(values: string[]): [string, string][] {
  return values.map((v) => [v, Number(v) % 60 === 0 ? `${Number(v) / 60}분` : `${v}초`]);
}

/// Settings in the order the screen shows them, by group.
export const SHOWN: Record<string, Shown> = {
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
    help: "다른 것을 복사했으면 지우지 않습니다.", above: (than) => `${than}초보다 길게 하면 확인합니다.`,
  },
  browser: {
    group: "브라우저", label: "세션을 여는 브라우저",
    control: { kind: "choices", choices: [["auto", "자동"], ["chrome", "Chrome"], ["ego-lite", "ego lite"]] },
    help: "자동: ego lite가 켜져 있으면 ego lite, 아니면 Chrome. ego lite 세션은 다른 프로그램도 같은 창을 조작할 수 있어 보호가 낮습니다.",
  },
  ego_probe_seconds: {
    group: "브라우저", label: "ego lite를 기다리는 시간",
    control: { kind: "choices", choices: [["3", "3초"], ["5", "5초"], ["10", "10초"]] },
    help: "자동일 때 이 시간 안에 ego lite가 답하지 않으면 Chrome으로 엽니다.",
  },
  ego_poll_ms: {
    group: "브라우저", label: "ego lite 페이지 확인 간격",
    control: { kind: "choices", choices: [["10", "10ms"], ["20", "20ms"], ["50", "50ms"], ["100", "100ms"]] },
    help: "짧을수록 빨리 반응하고 일을 더 합니다.",
  },
  upload_dirs: {
    group: "파일 올리기", label: "올릴 수 있는 폴더", control: { kind: "folders" },
    help: "에이전트가 사이트에 올릴 수 있는 파일은 이 폴더 안의 것뿐입니다. 비워 두면 올리지 못합니다. 바꿀 때는 확인합니다.",
  },
  upload_max_files: {
    group: "파일 올리기", label: "한 번에 올리는 파일 수",
    control: { kind: "choices", choices: [["5", "5개"], ["10", "10개"], ["20", "20개"]] },
    help: "", above: (than) => `${than}개보다 많게 하면 확인합니다.`,
  },
  notify_requests: { group: "알림", label: "정보 요청", control: { kind: "switch" }, help: "에이전트가 값을 입력해 달라고 할 때" },
  notify_handoffs: { group: "알림", label: "브라우저 인계", control: { kind: "switch" }, help: "에이전트가 로그인을 넘길 때" },
  notify_drafts: { group: "알림", label: "새 draft", control: { kind: "switch" }, help: "확정을 기다리는 항목이 생길 때" },
  notify_denials: { group: "알림", label: "거부", control: { kind: "switch" }, help: "정책이 에이전트의 사용을 막았을 때" },
  notify_locks: { group: "알림", label: "치환 잠김", control: { kind: "switch" }, help: "세션이 값 넣기를 멈췄을 때. 세션 화면에서 다시 허용합니다" },
  // 고급: waits and limits that were constants (#201, J-270–J-274).
  session_start_seconds: {
    group: "고급", label: "세션 시작을 기다리는 시간",
    control: { kind: "choices", choices: seconds(["15", "30", "60", "120"]) },
    help: "이 시간 안에 브라우저가 뜨지 않으면 세션 열기가 실패합니다. 느린 Mac에서 첫 실행이 걸리면 늘리세요.",
  },
  session_stop_seconds: {
    group: "고급", label: "세션 멈춤을 기다리는 시간",
    control: { kind: "choices", choices: seconds(["15", "30", "60"]) },
    help: "이 시간 안에 브라우저가 닫히지 않으면 멈춤이 실패로 보입니다.",
  },
  browser_endpoint_seconds: {
    group: "고급", label: "CDP 주소를 기다리는 시간",
    control: { kind: "choices", choices: seconds(["15", "30", "60"]) },
    help: "CDP를 켜고 연 세션에서 브라우저가 주소를 알려 줄 때까지 기다립니다.",
  },
  browser_call_seconds: {
    group: "고급", label: "브라우저 응답을 기다리는 시간",
    control: { kind: "choices", choices: seconds(["30", "60", "90"]) },
    help: "브라우저가 명령 하나에 이 시간 안에 답하지 않으면 그 명령이 실패합니다. 새로 여는 세션부터 적용됩니다.",
  },
  request_ttl_hours: {
    group: "고급", label: "정보 요청 만료",
    control: { kind: "choices", choices: [["1", "1시간"], ["8", "8시간"], ["24", "1일"], ["72", "3일"]] },
    help: "에이전트가 값을 입력해 달라고 한 요청은 이 시간이 지나면 답할 수 없습니다.",
    above: (than) => `${than}시간보다 길게 하면 확인합니다.`,
  },
  max_drafts: {
    group: "고급", label: "같은 이름의 draft 수",
    control: { kind: "choices", choices: [["10", "10개"], ["20", "20개"], ["50", "50개"]] },
    help: "에이전트가 같은 이름으로 쌓아 둘 수 있는 확정 전 항목 수입니다.",
    above: (than) => `${than}개보다 많게 하면 확인합니다.`,
  },
};

const GROUPS = ["보안", "브라우저", "파일 올리기", "새 항목과 draft", "클립보드", "알림", "고급"];

const set = (key: string, value: string) => `data-action="set-setting" data-key="${esc(key)}" data-value="${esc(value)}"`;

function control(s: Setting, c: Control): string {
  if (c.kind === "folders") return folders(s);
  if (c.kind === "switch") {
    const on = s.value === "true";
    return `<button type="button" class="kh-toggle${on ? " is-on" : ""}" role="switch" aria-checked="${on}" ${set(s.key, on ? "false" : "true")}><span class="track"></span></button>`;
  }
  // A value set elsewhere (the CLI takes any `<N>d`) stays visible as its own choice.
  const choices: [value: string, label: string][] = c.choices.some(([v]) => v === s.value) ? c.choices : [...c.choices, [s.value, label(s.value)]];
  const buttons = choices
    .map(([v, text]) => `<button type="button"${v === s.value ? ' class="is-on"' : ""} ${set(s.key, v)}>${esc(text)}</button>`)
    .join("");
  return `<div class="kh-seg" role="group">${buttons}</div>`;
}

/// `45d` → `45일`, anything else as it is.
const label = (value: string) => (/^\d+d$/.test(value) ? `${value.slice(0, -1)}일` : value);

/// The help line, with the weakening sentence first when the setting has one.
function help(shown: Shown, weakening: Weakening): string {
  const rule = weakening.when === "above" && shown.above ? shown.above(weakening.than) : "";
  return [rule, shown.help].filter(Boolean).join(" ");
}

function row(s: Setting, shown: Shown): string {
  return `<div class="setting"><div><div class="title">${esc(shown.label)}</div><div class="kh-help">${esc(help(shown, s.weakening))}</div></div>${control(s, shown.control)}</div>`;
}

/// A setting this table does not know yet: a plain field, so it can still be changed.
function plain(s: Setting): string {
  return `<form class="setting" data-setting="${esc(s.key)}"><div><div class="title">${esc(s.key)}</div><div class="kh-help">기본 ${esc(s.default)}</div></div>
    <span><input class="kh-input" name="value" value="${esc(s.value)}" autocomplete="off" spellcheck="false"> <button class="kh-btn secondary" type="submit">저장</button></span></form>`;
}

/// What the method in use (`auth`, from the app's authenticator) means for the person. Only
/// [`NO_AUTH`] exists (R-21); the others are shown, not chosen.
const AUTH_HELP: Record<string, string> = {
  [NO_AUTH]: "인증 없음: 이 Mac 사용자 계정으로 접근할 수 있는 누구나 금고를 볼 수 있습니다.",
};

function authSection(auth: string | null): string {
  const help = auth ? AUTH_HELP[auth] : undefined;
  return `<section class="settings-group"><h3>인증 방식</h3><div class="setting"><div><div class="title">앱과 값 보기 잠금</div>
    ${help ? `<div class="kh-help">${esc(help)}</div>` : ""}</div>
    <div class="kh-seg" role="group"><button type="button"${auth === NO_AUTH ? ' class="is-on"' : ""}>없음</button><button type="button" disabled>macOS 인증 · 준비 중</button><button type="button" disabled>마스터 비밀번호 · 준비 중</button></div></div></section>`;
}

export function settings(list: Setting[], browsers: Browsers = { ego_lite: false }, auth: string | null = null): string {
  const sections = GROUPS.map((group) => {
    // Whether ego lite answers right now goes with the browser choice it changes.
    const status = browsers.ego_lite
      ? " 지금 ego lite가 켜져 있습니다."
      : " 지금은 ego lite를 찾지 못했습니다(설치되지 않았거나 꺼져 있음).";
    const rows = list
      .flatMap((s) => {
        const shown = SHOWN[s.key];
        if (shown?.group !== group) return [];
        return [row(s, s.key === "browser" ? { ...shown, help: shown.help + status } : shown)];
      })
      .join("");
    return rows ? `<section class="settings-group"><h3>${group}</h3>${rows}</section>` : "";
  }).join("");
  const unknown = list.filter((s) => !SHOWN[s.key]).map(plain).join("");
  const others = unknown ? `<section class="settings-group"><h3>기타</h3>${unknown}</section>` : "";
  return `${heading("settings")}${sections}${others}${authSection(auth)}`;
}

/// How `upload_dirs` keeps its folders in one value, like `PATH`.
const FOLDER_SEPARATOR = ":";

/// The folders a list setting holds, without blanks.
export const parseFolders = (value: string): string[] => value.split(FOLDER_SEPARATOR).map((d) => d.trim()).filter(Boolean);

/// The value that holds `folders`, blanks left out.
export const joinFolders = (folders: string[]): string => folders.map((d) => d.trim()).filter(Boolean).join(FOLDER_SEPARATOR);

/// `upload_dirs` as a list: each folder can be taken out, one can be added. The new value is
/// the whole list, so a weakening change still goes through the question.
function folders(s: Setting): string {
  const list = parseFolders(s.value);
  const rows = list
    .map((dir) => {
      const rest = joinFolders(list.filter((d) => d !== dir));
      return `<li><code>${esc(dir)}</code> <button type="button" class="kh-btn ghost sm" ${set(s.key, rest)}>빼기</button></li>`;
    })
    .join("");
  return `<div class="folders">${rows ? `<ul class="folder-list">${rows}</ul>` : `<p class="kh-help">허용한 폴더가 없습니다</p>`}
    <form class="folder-add" data-setting="${esc(s.key)}" data-append="${esc(s.value)}"><input class="kh-input" name="value" placeholder="~/Documents/Recruiting" autocomplete="off" spellcheck="false"> <button class="kh-btn secondary sm" type="submit">추가</button></form></div>`;
}
