// Every component, its variants and its states side by side (forced is-* classes).
import { icon, each } from "../html.js";

const VARIANTS = ["primary", "secondary", "ghost", "danger"];
const STATES = [["default", ""], ["hover", "is-hover"], ["focus", "is-focus"], ["active", "is-active"], ["disabled", "is-disabled"], ["loading", "is-loading"]];

const buttonMatrix = () => `<div class="kh-panel"><div class="matrix" id="btnmatrix"><span></span>${each(STATES, s => `<span class="h">${s[0]}</span>`)}`
  + each(VARIANTS, v => `<span class="h">${v}</span>` + each(STATES, s => `<button class="kh-btn ${v} ${s[1]}">${icon(v === "danger" ? "trash-2" : "key-round")}Label</button>`))
  + `</div></div>`;

export const components = {
  id: "components",
  title: ["컴포넌트", "Components"],
  lead: ["상태는 강제 클래스(is-hover, is-focus…)로 나란히 비교합니다.", "States are shown with forced classes (is-hover, is-focus…) so they can be compared side by side."],
  parts: [
    { title: "Button", body: buttonMatrix },
    { title: "IconButton", body: () => `
<div class="kh-panel row">
  <button class="kh-ibtn"><span class="kh-i" style="mask-image:url(icons/copy.svg)"></span></button>
  <button class="kh-ibtn is-hover"><span class="kh-i" style="mask-image:url(icons/eye.svg)"></span></button>
  <button class="kh-ibtn is-pressed"><span class="kh-i" style="mask-image:url(icons/eye-off.svg)"></span></button>
  <button class="kh-ibtn danger is-hover"><span class="kh-i" style="mask-image:url(icons/trash-2.svg)"></span></button>
  <button class="kh-ibtn" disabled><span class="kh-i" style="mask-image:url(icons/refresh-ccw-dot.svg)"></span></button>
</div>` },
    { title: ["Input · 비밀 필드", "Input · secret field"], body: () => `
<div class="grid">
  <div class="kh-panel stack">
    <label class="kh-field"><span class="kh-label" data-en="Item name">항목 이름</span><input class="kh-input" placeholder="github"></label>
    <label class="kh-field"><span class="kh-label" data-en="Focused">포커스</span><input class="kh-input is-focus" value="openai"></label>
    <label class="kh-field"><span class="kh-label" data-en="Error">오류</span><input class="kh-input is-error" value="git hub"><span class="kh-help error" data-en="Use letters, digits, - and _">영문, 숫자, -, _만 씁니다</span></label>
    <label class="kh-field"><span class="kh-label" data-en="Read only">읽기 전용</span><input class="kh-input" value="login" readonly></label>
  </div>
  <div class="kh-panel stack">
    <label class="kh-field"><span class="kh-label" data-en="Sealed">봉인</span><div class="kh-secret"><span class="value">••••••••••••</span><button class="kh-ibtn"><span class="kh-i" style="mask-image:url(icons/copy.svg)"></span></button></div></label>
    <label class="kh-field"><span class="kh-label" data-en="Shown in the admin app, hides itself">관리 앱에서 보기 (스스로 다시 가림)</span><div class="kh-secret is-revealed"><span class="value">correct-horse-battery</span><span class="timer">0:08</span><button class="kh-ibtn"><span class="kh-i" style="mask-image:url(icons/eye-off.svg)"></span></button></div></label>
    <span class="kh-help" data-en="Secrets never render on a plain surface. The CLI has no way to show them.">비밀 값은 일반 표면에 그리지 않습니다. CLI에는 보여 주는 방법이 없습니다.</span>
    <label class="kh-field"><span class="kh-label" data-en="Note">메모</span><textarea class="kh-textarea" placeholder="이 항목을 에이전트가 언제 쓰는지" data-en-ph="When an agent should use this item"></textarea></label>
  </div>
</div>` },
    { title: "Toggle · Segmented · Tabs", body: () => `
<div class="kh-panel stack">
  <div class="row">
    <label class="kh-toggle is-on"><span class="track"></span><t data-en="Origin binding">origin 고정</t></label>
    <label class="kh-toggle"><span class="track"></span><t data-en="Allow in run">run에서 허용</t></label>
    <label class="kh-toggle is-on is-disabled"><span class="track"></span><t data-en="Usage log (locked)">사용 기록 (잠김)</t></label>
  </div>
  <div class="row">
    <div class="kh-seg"><button class="is-on"><t data-en="All">전체</t></button><button><t data-en="Allowed">허용</t></button><button><t data-en="Denied">거부</t></button></div>
  </div>
  <div class="kh-tabs"><button class="is-on"><span class="kh-i" style="mask-image:url(icons/key.svg)"></span><t data-en="Fields">필드</t></button><button><span class="kh-i" style="mask-image:url(icons/globe.svg)"></span><t data-en="Sites">사이트</t></button><button><span class="kh-i" style="mask-image:url(icons/scroll-text.svg)"></span><t data-en="History">기록</t></button></div>
</div>` },
    { title: ["Chip · Badge · 도메인 · 상태", "Chips · badges · domains · states"], body: () => `
<div class="kh-panel stack">
  <div class="row">
    <span class="kh-chip session"><span class="kh-i" style="mask-image:url(icons/globe.svg)"></span><t data-en="session">세션</t></span>
    <span class="kh-chip cred"><span class="kh-i" style="mask-image:url(icons/key-round.svg)"></span><t data-en="credential">자격 증명</t></span>
    <span class="kh-chip pay"><span class="kh-i" style="mask-image:url(icons/credit-card.svg)"></span><t data-en="payment">결제</t></span>
    <span class="kh-chip personal"><span class="kh-i" style="mask-image:url(icons/id-card.svg)"></span><t data-en="personal">개인 정보</t></span>
    <span class="kh-chip dev"><span class="kh-i" style="mask-image:url(icons/square-terminal.svg)"></span><t data-en="developer">개발자 비밀</t></span>
    <span class="kh-chip agent"><span class="dot"></span>claude-code</span>
    <span class="kh-chip">env</span>
  </div>
  <div class="row">
    <span class="kh-badge active" data-en="active">사용 중</span>
    <span class="kh-badge draft" data-en="draft">초안</span>
    <span class="kh-badge expiring" data-en="expiring">곧 만료</span>
    <span class="kh-badge expired" data-en="expired">만료</span>
    <span class="kh-badge denied" data-en="denied">거부</span>
    <span class="kh-badge tampered"><span class="kh-i" style="mask-image:url(icons/triangle-alert.svg);width:11px;height:11px"></span><t data-en="tampered">변조</t></span>
    <span class="kh-badge neutral">locked</span>
    <span class="kh-count">3</span><span class="kh-count muted">12</span><span class="kh-kbd">⌘</span><span class="kh-kbd">K</span>
  </div>
</div>` },
    { title: ["Placeholder · 허용 위치", "Placeholder · origins"], body: () => `
<div class="kh-panel stack">
  <div class="row"><span class="kh-ph"><span class="kh-i" style="mask-image:url(icons/braces.svg)"></span>{{secret:github.password}}</span><span class="kh-ph"><span class="kh-i" style="mask-image:url(icons/braces.svg)"></span>{{secret:visa-main.number}}</span></div>
  <span class="kh-help" data-en="What an agent sees and types. Safe to copy; the value is filled in only at an allowed site.">에이전트가 보고 입력하는 것. 복사해도 안전하고, 값은 허용된 사이트에서만 채워집니다.</span>
  <div class="kh-origins"><span class="site"><span class="kh-i" style="mask-image:url(icons/globe.svg)"></span>https://github.com</span><span class="site"><span class="kh-i" style="mask-image:url(icons/globe.svg)"></span>https://gist.github.com</span><span class="state"><span class="kh-i" style="mask-image:url(icons/shield-check.svg)"></span><t data-en="bound">고정됨</t></span></div>
  <div class="kh-origins is-off"><span class="site" data-en="Any site">모든 사이트</span><span class="state"><span class="kh-i" style="mask-image:url(icons/triangle-alert.svg)"></span><t data-en="not bound">고정 안 됨</t></span></div>
</div>` },
    { title: "Toast · Menu", body: () => `
<div class="grid wide">
  <div class="stack">
    <div class="kh-toast ok"><span class="kh-i lead" style="mask-image:url(icons/check.svg)"></span><span class="msg" data-en="Placeholder copied">placeholder를 복사했습니다</span></div>
    <div class="kh-toast warn"><span class="kh-i lead" style="mask-image:url(icons/clock.svg)"></span><span class="msg" data-en="openai expires in 12 days">openai가 12일 뒤 만료됩니다</span><button class="act" data-en="Extend">연장</button></div>
    <div class="kh-toast danger"><span class="kh-i lead" style="mask-image:url(icons/shield-x.svg)"></span><span class="msg" data-en="Blocked: github.password at gist-evil.io">차단: gist-evil.io에서 github.password</span><button class="act" data-en="View">보기</button></div>
  </div>
  <div class="kh-menu">
    <div class="item"><span class="kh-i" style="mask-image:url(icons/copy.svg)"></span><t data-en="Copy placeholder">placeholder 복사</t><span class="kh-kbd">C</span></div>
    <div class="item is-hover"><span class="kh-i" style="mask-image:url(icons/file-pen-line.svg)"></span><t data-en="Edit">편집</t><span class="kh-kbd">E</span></div>
    <div class="item"><span class="kh-i" style="mask-image:url(icons/refresh-ccw-dot.svg)"></span><t data-en="Rotate">값 교체</t></div>
    <div class="item is-disabled"><span class="kh-i" style="mask-image:url(icons/download.svg)"></span><t data-en="Export (unlock first)">내보내기 (먼저 잠금 해제)</t></div>
    <hr>
    <div class="item danger"><span class="kh-i" style="mask-image:url(icons/trash-2.svg)"></span><t data-en="Delete">삭제</t></div>
  </div>
</div>` },
    { title: ["빈 상태", "Empty state"], body: () => `
<div class="kh-panel"><div class="kh-empty"><div class="art"><span class="kh-i xl" style="mask-image:url(icons/inbox.svg)"></span></div><h4 data-en="Nothing in the harbor yet">아직 항구가 비어 있습니다</h4><p style="margin:0" data-en="Add a login, or import a .env file. Values stay on this Mac.">로그인을 추가하거나 .env 파일을 가져오세요. 값은 이 Mac에만 남습니다.</p><div class="row"><button class="kh-btn primary"><span class="kh-i" style="mask-image:url(icons/plus.svg)"></span><t data-en="Add item">항목 추가</t></button><button class="kh-btn secondary"><span class="kh-i" style="mask-image:url(icons/upload.svg)"></span><t data-en="Import .env">.env 가져오기</t></button></div></div></div>` },
    { title: ["행", "Rows"], body: () => `
<div class="grid wide">
  <div class="kh-panel" style="padding:var(--space-2)">
    <div class="kh-row"><div class="glyph session"><span class="kh-i" style="mask-image:url(icons/globe.svg)"></span></div><div class="main"><div class="title">work</div><div class="sub">Chrome · 3 tabs</div></div><span class="kh-badge active" data-en="running">실행 중</span></div>
    <div class="kh-row is-hover"><div class="glyph session"><span class="kh-i" style="mask-image:url(icons/app-window.svg)"></span></div><div class="main"><div class="title">shopping</div><div class="sub" data-en="handed off to you">사용자에게 넘김</div></div><span class="kh-badge draft"><span class="kh-i" style="mask-image:url(icons/hand.svg);width:11px;height:11px"></span>handoff</span></div>
    <div class="kh-row"><div class="glyph session"><span class="kh-i" style="mask-image:url(icons/circle-stop.svg)"></span></div><div class="main"><div class="title">research</div><div class="sub" data-en="stopped 2h ago">2시간 전 정지</div></div><span class="meta">2h</span></div>
  </div>
  <div class="kh-panel" style="padding:0">
    <div class="kh-audit"><span class="t">14:02</span><span class="kh-chip agent"><span class="dot"></span>claude</span><span class="what">browser_type github.password</span><span class="kh-badge active">ok</span></div>
    <div class="kh-audit"><span class="t">13:58</span><span class="kh-chip agent"><span class="dot"></span>codex</span><span class="what">run --env openai -- pytest</span><span class="kh-badge active">ok</span></div>
    <div class="kh-audit"><span class="t">13:40</span><span class="kh-chip agent"><span class="dot"></span>claude</span><span class="what">browser_type github.password @ gist-evil.io</span><span class="kh-badge denied">origin</span></div>
    <div class="kh-audit"><span class="t">13:12</span><span class="kh-chip">you</span><span class="what">vault edit github --origin +gist.github.com</span><span class="kh-badge neutral" data-en="confirmed">확인됨</span></div>
  </div>
</div>` },
    { title: ["보안 입력 창", "Secure input window"], body: () => `
<div class="kh-scrim">
  <div class="kh-dialog secure">
    <div class="body">
      <div class="who"><span class="kh-i lg" style="mask-image:url(icons/bot.svg)"></span><div><b>claude-code</b> <t data-en="asks you to enter">이(가) 입력을 요청합니다</t><div class="kh-help" style="margin-top:2px">github · password</div></div></div>
      <label class="kh-field"><span class="kh-label">password</span><input class="kh-input is-focus" type="password" value="hunter2hunter2"></label>
      <div class="kh-sealbar"><span class="kh-i" style="mask-image:url(icons/lock.svg)"></span><t data-en="Typed here, sealed here. The agent only gets a placeholder.">여기서 입력하고 여기서 봉인합니다. 에이전트는 placeholder만 받습니다.</t></div>
    </div>
    <div class="foot"><button class="kh-btn ghost" data-en="Cancel">취소</button><button class="kh-btn primary"><span class="kh-i" style="mask-image:url(icons/lock.svg)"></span><t data-en="Seal">봉인</t></button></div>
  </div>
</div>` },
  ],
};
