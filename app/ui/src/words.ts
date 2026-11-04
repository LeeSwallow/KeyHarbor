// What the window says for the values the commands return (09 §9.11: the person reads words,
// not stored names). A value missing from a table is shown as it is, so a new one is still
// readable before it gets a word here. Environment variable names are never translated.

import type { SiteHandOff } from "./api.js";

const word = (table: Record<string, string>) => (value: string) => table[value] ?? value;

/// The sidebar's security line, by authentication method (03 §3.10, R-21).
export const authWord = word({ none: "인증 없음" });

export const kindWord = word({
  login: "로그인", secret: "API 키", env: "환경 변수", card: "카드", identity: "신원", address: "주소",
});

/// Each kind's domain glyph (design system `.glyph.<domain>`) and icon.
export const kindLook: Record<string, { domain: string; icon: string }> = {
  login: { domain: "cred", icon: "key" },
  secret: { domain: "dev", icon: "braces" },
  env: { domain: "dev", icon: "square-terminal" },
  card: { domain: "pay", icon: "credit-card" },
  identity: { domain: "personal", icon: "id-card" },
  address: { domain: "personal", icon: "map-pin" },
};

export const fieldWord = word({
  username: "아이디", password: "비밀번호", totp_seed: "OTP 시드", note: "메모", value: "값",
  number: "카드 번호", exp_month: "만료 월", exp_year: "만료 연도", cvc: "CVC", holder: "명의자", billing_address: "청구 주소",
  full_name: "성명", given_name: "이름", family_name: "성", email: "이메일", phone: "전화번호", birth_date: "생년월일",
  english_name: "영문 이름", hanja_name: "한자 이름", gender: "성별", nationality: "국적", emergency_phone: "비상 연락처",
  military_service: "병역", disability: "장애", veterans: "보훈",
  recipient: "받는 사람", line1: "주소", line2: "상세 주소", city: "시·군·구", region: "시·도", postal_code: "우편번호", country: "국가",
});

export const sourceWord = word({
  user: "직접 입력", import: "가져옴", generated: "생성됨", captured: "포착됨", agent: "에이전트", requested: "요청으로 받음",
});

export const sessionWord = word({
  running: "실행 중", stopped: "멈춤", starting: "시작 중", stopping: "멈추는 중", awaiting_user: "사람 기다림",
  failed: "실패", recovery_required: "복구 필요",
});

/// How a site dragged onto `session` ended (`SiteHandOff`, J-202).
export function handOffWord(answer: SiteHandOff, session: string): string {
  switch (answer.outcome) {
    case "handed_off": return `쿠키 ${answer.cookies}개를 ${session} 세션에 넘겼습니다`;
    case "declined": return "브라우저에서 넘기지 않았습니다";
    case "no_permission": return "확장이 이 사이트에 접근할 수 없습니다. 확장 팝업에서 한 번 넘겨 허용하세요";
    case "no_session": return `실행 중인 ${session} 세션이 없습니다`;
    case "bad_site": return "http·https 사이트만 넘길 수 있습니다";
    case "timeout": return "브라우저에서 답이 오지 않았습니다";
    case "failed": return "넘기지 못했습니다";
  }
}

export const siteWord = word({ logged_in: "로그인됨", expired: "로그인 만료", unknown: "확인 안 됨" });

/// Every `Resource` (test/vocabulary.test.ts checks it against Rust, J-175).
export const resourceWords: Record<string, string> = {
  item: "항목", domain: "도메인", session: "세션", create_session: "세션 만들기", raw_cdp: "raw CDP", browser: "브라우저",
};
export const resourceWord = word(resourceWords);

export const effectWord = word({ allow: "허용", deny: "거부" });

export const domainWord = word({
  credentials: "자격 증명", payment: "결제", personal: "개인정보", developer: "개발자", "*": "모두",
});

/// Every `AuditKind` (test/vocabulary.test.ts checks it against Rust, J-175).
export const auditWords: Record<string, string> = {
  secret_used: "값 사용", secret_denied: "사용 거부", secret_denied_suppressed: "사용 거부(반복)",
  origin_changed_after_fill: "입력 뒤 사이트 바뀜", session_opened: "세션 열림", session_stopped: "세션 멈춤",
  session_removed: "세션 지움", file_uploaded: "파일 올림",
  handoff_requested: "인계 요청", handoff_resumed: "인계 재개", policy_changed: "정책 변경", vault_item_changed: "항목 변경",
  env_injected: "변수 주입", substitution_locked: "치환 잠김", mask_probe: "마스킹 탐지", first_use_origin: "처음 쓰는 사이트",
  site_state_changed: "로그인 상태 바뀜", vault_requested: "정보 요청", vault_request_answered: "요청 답함",
  credential_captured: "로그인 포착", grant_changed: "grant 변경", navigation_denied: "이동 거부", site_handed_off: "사이트 넘김",
  value_revealed: "값 보기", value_autofilled: "자동 완성",
};
export const auditWord = word(auditWords);
