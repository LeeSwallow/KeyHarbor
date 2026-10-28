// The admin app window, built only from components and the app-shell pattern.
import { icon, t, each } from "../html.js";

const NAV = [
  { group: ["도메인", "Domains"] },
  { icon: "globe", label: ["세션", "Sessions"], count: 3 },
  { icon: "key-round", label: ["자격 증명", "Credentials"], count: 6, on: true },
  { icon: "square-terminal", label: ["개발자 비밀", "Developer"], count: 5 },
  { icon: "credit-card", label: ["결제", "Payment"], count: 2 },
  { icon: "id-card", label: ["개인 정보", "Personal"], count: 8 },
  { group: ["살피기", "Watch"] },
  { icon: "scroll-text", label: ["사용 기록", "Usage log"] },
  { icon: "shield-check", label: ["정책", "Policy"] },
  { icon: "settings", label: ["설정", "Settings"] },
];

const ITEMS = [
  { dom: "cred", icon: "key-round", title: "github", sub: "login · github.com", badge: ["active", "사용 중", "active"], selected: true },
  { dom: "dev", icon: "braces", title: "openai", sub: "env · 2 keys", badge: ["expiring", "12일", "12 days"] },
  { dom: "pay", icon: "credit-card", title: "visa-main", sub: "card · •••• 4242" },
  { dom: "personal", icon: "map-pin", title: "home", sub: "address" },
  { dom: "dev", icon: "key-square", title: "stripe-test", sub: "secret", badge: ["expired", "만료", "expired"] },
];

const navEntry = n => n.group
  ? `<div class="group" data-en="${n.group[1]}">${n.group[0]}</div>`
  : `<div class="nav${n.on ? " is-on" : ""}">${icon(n.icon)}${t(...n.label)}${n.count ? `<span class="kh-count${n.on ? "" : " muted"}">${n.count}</span>` : ""}</div>`;

const itemRow = i => `<div class="kh-row${i.selected ? " is-selected" : ""}"><div class="glyph ${i.dom}">${icon(i.icon)}</div>`
  + `<div class="main"><div class="title">${i.title}</div><div class="sub">${i.sub}</div></div>`
  + (i.badge ? `<span class="kh-badge ${i.badge[0]}" data-en="${i.badge[2]}">${i.badge[1]}</span>` : "") + `</div>`;

const detail = () => `<section class="detail kh-canvas">
  <div class="kh-itemhead"><div class="kh-row" style="padding:0;min-height:0"><div class="glyph cred">${icon("key-round", "lg")}</div></div><h3>github</h3>
    <button class="kh-btn secondary sm">${icon("file-pen-line")}${t("편집", "Edit")}</button>
    <button class="kh-ibtn danger" aria-label="delete">${icon("trash-2")}</button></div>
  <div class="kv">
    <span>username</span><input class="kh-input" value="leeswallow" readonly>
    <span>password</span><div class="kh-secret"><span class="value">••••••••••••</span><button class="kh-ibtn" aria-label="copy placeholder">${icon("copy")}</button></div>
    <span data-en="Agent uses">에이전트 사용</span><span class="kh-ph">${icon("braces")}{{secret:github.password}}</span>
    <span data-en="Allowed at">허용 위치</span><div class="kh-origins"><span class="site">${icon("globe")}https://github.com</span><span class="state">${icon("shield-check")}${t("고정됨", "bound")}</span></div>
  </div>
  <div>
    <h3 class="sub" style="margin-top:0" data-en="Recent use">최근 사용</h3>
    <div class="kh-audit"><span class="t">14:02</span><span class="kh-chip agent"><span class="dot"></span>claude</span><span class="what">fill github.password → github.com</span><span class="kh-badge active">ok</span></div>
    <div class="kh-audit"><span class="t">13:40</span><span class="kh-chip agent"><span class="dot"></span>claude</span><span class="what">fill github.password → gist-evil.io</span><span class="kh-badge denied">origin</span></div>
  </div>
</section>`;

export const composed = {
  id: "composed",
  title: ["조합 예시", "Composed"],
  lead: ["관리 앱을 아래 부품만으로 조립한 모습입니다.", "The admin app, built only from the parts below."],
  body: () => `<div class="kh-window">
  <div class="kh-titlebar"><div class="kh-lights"><i></i><i></i><i></i></div>
    <div class="kh-inputwrap" style="flex:1;max-width:320px;margin:0 auto">${icon("search")}<input class="kh-input" style="height:26px" placeholder="항목, 사이트, 세션 검색" data-en-ph="Search items, sites, sessions"></div>
    <span class="kh-chip agent"><span class="dot"></span>claude-code</span>
    <button class="kh-ibtn" aria-label="lock">${icon("lock")}</button>
  </div>
  <div class="kh-shell">
    <nav class="side"><div class="nav">${icon("inbox")}${t("모든 항목", "All items")}<span class="kh-count muted">24</span></div>${each(NAV, navEntry)}</nav>
    <div class="list">${each(ITEMS, itemRow)}</div>
    ${detail()}
  </div>
</div>`,
};
