// Popup text in Korean and English. Every key exists in both; test/ui checks it.

import type { ErrorCode } from "../../domain/result.js";

export type Lang = "ko" | "en";

export const pickLanguage = (navigatorLanguage: string): Lang => (navigatorLanguage.toLowerCase().startsWith("ko") ? "ko" : "en");

const KO = {
  title: "사이트 넘기기",
  scope: "이 사이트와 하위 도메인",
  session: "에이전트 세션",
  ttl: "기간",
  ttl3600: "1시간",
  ttl28800: "8시간",
  ttl86400: "24시간",
  localStorage: "localStorage도 넘기기",
  note: "쿠키 값은 이 창에도 로그에도 나오지 않습니다. KeyHarbor의 전용 프로필로 곧바로 들어갑니다.",
  submit: "에이전트 세션에 넘기기",
  working: "넘기는 중…",
  done: "넘겼습니다",
  doneDetail: "쿠키 {n}개 → {session}",
  unsupportedTitle: "이 페이지는 넘길 수 없습니다",
  unsupportedBody: "http·https 사이트에서만 쓸 수 있습니다.",
} as const;

export type MessageKey = keyof typeof KO;

const EN: Record<MessageKey, string> = {
  title: "Hand off this site",
  scope: "this site and its subdomains",
  session: "Agent session",
  ttl: "For",
  ttl3600: "1 hour",
  ttl28800: "8 hours",
  ttl86400: "24 hours",
  localStorage: "Also hand off localStorage",
  note: "Cookie values never appear here or in a log. They go straight into KeyHarbor's own profile.",
  submit: "Hand off to the agent session",
  working: "Handing off…",
  done: "Handed off",
  doneDetail: "{n} cookies → {session}",
  unsupportedTitle: "This page can't be handed off",
  unsupportedBody: "Only http and https sites can be.",
};

export const MESSAGES: Record<Lang, Record<MessageKey, string>> = { ko: KO, en: EN };

export const ERRORS: Record<Lang, Record<ErrorCode, string>> = {
  ko: {
    unsupported_page: "이 페이지는 넘길 수 없습니다.",
    bad_session: "세션 이름은 영문 소문자, 숫자, -, _로 64자까지입니다.",
    bad_ttl: "기간이 범위를 벗어났습니다.",
    no_permission: "이 사이트에 대한 접근을 허용해야 넘길 수 있습니다.",
    no_cookies: "넘길 로그인 쿠키가 없습니다.",
    host_unavailable: "KeyHarbor 네이티브 호스트가 없습니다. keyharbor browser install-host를 실행하세요.",
    host_refused: "KeyHarbor가 거절했습니다. 세션이 실행 중인지 확인하세요.",
    bad_reply: "KeyHarbor의 응답을 읽지 못했습니다.",
    storage_unavailable: "이 페이지의 localStorage를 읽지 못했습니다.",
    bad_command: "요청을 처리하지 못했습니다.",
  },
  en: {
    unsupported_page: "This page can't be handed off.",
    bad_session: "Session names are lowercase letters, digits, - and _, up to 64.",
    bad_ttl: "That length is out of range.",
    no_permission: "Allow access to this site to hand it off.",
    no_cookies: "There are no login cookies to hand off.",
    host_unavailable: "The KeyHarbor native host is missing. Run keyharbor browser install-host.",
    host_refused: "KeyHarbor refused. Check that the session is running.",
    bad_reply: "KeyHarbor's answer could not be read.",
    storage_unavailable: "This page's localStorage could not be read.",
    bad_command: "The request could not be handled.",
  },
};

export const format = (text: string, values: Record<string, string | number>): string =>
  text.replace(/\{(\w+)\}/g, (_, k: string) => String(values[k] ?? ""));
