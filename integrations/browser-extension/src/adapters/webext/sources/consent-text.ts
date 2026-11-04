// The words of the hand-off notification, in Korean and English. Every language has every entry.

import type { Lang } from "../../../ui/popup/i18n.js";

/** How the person says yes: Chrome's notifications have a button, Firefox's are clicked. */
export type ConsentGesture = "button" | "click";

interface ConsentText {
  title: string;
  ask: (origin: string, session: string) => string;
  /** Appended to `ask` when there is no button to press. */
  clickHint: string;
  button: string;
}

export const CONSENT_TEXT: Record<Lang, ConsentText> = {
  ko: {
    title: "KeyHarbor: 사이트 넘기기",
    ask: (origin, session) => `${origin}의 로그인을 ${session} 세션에 넘길까요?`,
    clickHint: " 이 알림을 누르면 넘깁니다.",
    button: "넘기기",
  },
  en: {
    title: "KeyHarbor: hand off a site",
    ask: (origin, session) => `Hand your ${origin} login to the ${session} session?`,
    clickHint: " Click this notification to hand it off.",
    button: "Hand off",
  },
};

/** The notification's body: the question, and how to answer it when there is no button. */
export function consentMessage(lang: Lang, gesture: ConsentGesture, origin: string, session: string): string {
  const text = CONSENT_TEXT[lang];
  return text.ask(origin, session) + (gesture === "click" ? text.clickHint : "");
}
