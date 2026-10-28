// Renders the popup from its state, using only design-system classes (design-system/components,
// design-system/patterns/extension-popup.css). Pure: state in, HTML out. Values are escaped.

import { TTL_CHOICES } from "../../domain/handoff.js";
import type { ErrorCode } from "../../domain/result.js";
import { ERRORS, type Lang, MESSAGES, type MessageKey, format } from "./i18n.js";

export type PopupState =
  | { kind: "loading" }
  | { kind: "unsupported" }
  | { kind: "ready"; origin: string; session: string; ttlSeconds: number; includeLocalStorage: boolean; busy: boolean; error?: ErrorCode; done?: { cookies: number; session: string } };

const ESCAPES: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
export const escape = (s: string): string => s.replace(/[&<>"']/g, (c) => ESCAPES[c] ?? c);

/** An icon from design-system/icons (copied into the build as design/icons), drawn by kh-i. */
export const icon = (name: string, cls = ""): string => `<span class="kh-i${cls ? ` ${cls}` : ""}" style="mask-image:url(design/icons/${name}.svg)"></span>`;

const head = (t: (k: MessageKey) => string) =>
  `<div class="head"><span class="brand">${icon("lock", "lg")}KeyHarbor</span><span class="kh-badge neutral">${escape(t("title"))}</span></div>`;

function ready(s: Extract<PopupState, { kind: "ready" }>, lang: Lang): string {
  const t = (k: MessageKey) => MESSAGES[lang][k];
  const ttl = TTL_CHOICES.map(
    (v) => `<button type="button" data-ttl="${v}"${v === s.ttlSeconds ? ' class="is-on"' : ""}>${escape(t(`ttl${v}` as MessageKey))}</button>`,
  ).join("");
  const toast = s.error
    ? `<div class="kh-toast danger" role="alert">${icon("shield-x", "lead")}<span class="msg">${escape(ERRORS[lang][s.error])}</span></div>`
    : s.done
      ? `<div class="kh-toast ok" role="status">${icon("check", "lead")}<span class="msg"><b>${escape(t("done"))}</b> · ${escape(format(t("doneDetail"), { n: s.done.cookies, session: s.done.session }))}</span></div>`
      : "";
  return `<div class="kh-dialog">
  <div class="body">
    ${head(t)}
    <div class="kh-origins"><span class="site">${icon("globe")}${escape(s.origin)}</span><span class="state">${icon("shield-check")}${escape(t("scope"))}</span></div>
    <div class="opts">
      <label class="kh-field"><span class="kh-label">${escape(t("session"))}</span>
        <span class="kh-inputwrap">${icon("bot")}<input class="kh-input" id="session" value="${escape(s.session)}" autocomplete="off" spellcheck="false"></span></label>
      <div class="kh-field"><span class="kh-label">${escape(t("ttl"))}</span><div class="kh-seg" id="ttl">${ttl}</div></div>
      <label class="kh-toggle${s.includeLocalStorage ? " is-on" : ""}" id="ls"><span class="track"></span>${escape(t("localStorage"))}</label>
    </div>
    <div class="kh-sealbar">${icon("eye-off")}<span>${escape(t("note"))}</span></div>
    ${toast}
  </div>
  <div class="foot"><button class="kh-btn primary${s.busy ? " is-loading" : ""}" id="submit"${s.busy ? " disabled" : ""}>${s.busy ? icon("refresh-ccw-dot") : icon("hand")}${escape(t(s.busy ? "working" : "submit"))}</button></div>
</div>`;
}

export function renderPopup(state: PopupState, lang: Lang): string {
  const t = (k: MessageKey) => MESSAGES[lang][k];
  switch (state.kind) {
    case "loading":
      return `<div class="kh-panel">${head(t)}</div>`;
    case "unsupported":
      return `<div class="kh-panel">${head(t)}<div class="kh-empty"><div class="art">${icon("shield-x", "xl")}</div><h4>${escape(t("unsupportedTitle"))}</h4><p>${escape(t("unsupportedBody"))}</p></div></div>`;
    case "ready":
      return ready(state, lang);
  }
}
