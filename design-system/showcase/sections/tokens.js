// The semantic tokens, drawn from the live CSS variables so the page always matches the theme.
import { each } from "../html.js";

const COLORS = ["accent", "accent-strong", "accent-fill", "accent-wash", "bg", "surface", "surface-2", "surface-3", "line", "line-strong", "ink", "ink-2", "ink-3", "ink-4",
  "dom-session", "dom-cred", "dom-pay", "dom-personal", "dom-dev", "ok", "warn", "danger", "info", "sealed", "sealed-ink"];
const PATTERNS = [["graph", "모눈 · 8px + 40px"], ["grid", "격자 · 24px"], ["dots", "점 · 16px"], ["none", "없음"]];
const TYPE = ["display", "title", "heading", "body-strong", "body", "small", "label", "mono", "mono-sm"];
const SPACE = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
const RADIUS = ["xs", "sm", "md", "lg", "window", "pill"];
const SHADOW = [1, 2, 3];

export const tokens = {
  id: "tokens",
  title: ["토큰", "Tokens"],
  lead: ["위의 모든 값은 tokens/semantic.css에서 옵니다. 컴포넌트가 아니라 토큰을 바꿉니다.", "Every value above comes from tokens/semantic.css. Change the token, not the component."],
  parts: [
    { title: ["색", "Colour"], body: () => `<div class="sw" id="colors">${each(COLORS, c => `<div><i style="background:var(--c-${c})"></i><span>--c-${c}</span></div>`)}</div>` },
    { title: ["배경 패턴", "Background pattern"], body: () => `<div class="grid" id="patterns">${each(PATTERNS, ([v, label]) =>
      `<div data-pattern="${v}" class="kh-canvas pattern-sample"><span class="kh-chip">${v} — ${label}</span></div>`)}</div>` },
    { title: ["글자", "Type"], body: () => `<div class="kh-panel scale" id="type">${each(TYPE, n =>
      `<div><code>--text-${n}</code><span style="font:var(--text-${n})">항구에 열쇠를 맡기다 Harbor 123</span></div>`)}</div>` },
    { title: ["간격 · 반경 · 그림자", "Space · radius · elevation"], body: () => `<div class="grid">
  <div class="kh-panel scale" id="space">${each(SPACE, n => `<div><code>--space-${n}</code><span class="bar" style="width:var(--space-${n})"></span></div>`)}</div>
  <div class="kh-panel row" id="radius">${each(RADIUS, r => `<div class="sample"><div class="box radius" style="border-radius:var(--radius-${r})"></div><code>${r}</code></div>`)}${each(SHADOW, s =>
    `<div class="sample"><div class="box shadow" style="box-shadow:var(--shadow-${s})"></div><code>shadow-${s}</code></div>`)}</div>
</div>` },
  ],
};
