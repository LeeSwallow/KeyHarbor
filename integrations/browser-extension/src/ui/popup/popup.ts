// Mounts the popup: draws each state and sends each event to its handler (controller.ts). It
// never sees a cookie.

import { type PopupController, makePopupController } from "./controller.js";
import type { PopupDeps } from "./deps.js";
import { type Lang, pickLanguage } from "./i18n.js";
import { type PopupState, renderPopup } from "./view.js";

const CLICKABLE = "[data-ttl], #ls, #submit, [data-fill], #save-login";

export async function mountPopup(doc: Document, deps: PopupDeps): Promise<void> {
  const root = doc.getElementById("app");
  if (!root) return;
  const lang = pickLanguage(deps.language);
  doc.documentElement.lang = lang;
  const popup = makePopupController(deps, drawInto(root, lang));
  root.addEventListener("input", (e) => {
    const target = e.target as HTMLInputElement;
    if (target.id === "session") popup.onSession(target.value);
  });
  root.addEventListener("click", (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>(CLICKABLE);
    if (el) void dispatchClick(popup, el);
  });
  await popup.start();
}

/** The HTML is built from escaped values only (view.ts); parsed rather than assigned to innerHTML. */
const drawInto = (root: HTMLElement, lang: Lang) => (state: PopupState) => {
  const parsed = new DOMParser().parseFromString(renderPopup(state, lang), "text/html");
  root.replaceChildren(...parsed.body.childNodes);
};

function dispatchClick(popup: PopupController, el: HTMLElement): Promise<void> | void {
  if (el.dataset.fill) return popup.onFill(el.dataset.fill);
  if (el.id === "save-login") return popup.onSaveLogin();
  if (el.dataset.ttl) return popup.onToggle({ kind: "ttl", seconds: Number(el.dataset.ttl) });
  if (el.id === "ls") return popup.onToggle({ kind: "localStorage" });
  return popup.onHandOff();
}
