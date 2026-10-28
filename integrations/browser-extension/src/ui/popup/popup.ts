// The popup controller: state, events, and the one call to the background. It never sees a cookie.

import { TTL_CHOICES } from "../../domain/handoff.js";
import { hostPatterns, scopeOf } from "../../domain/scope.js";
import type { PopupDeps } from "./deps.js";
import { pickLanguage } from "./i18n.js";
import { type PopupState, renderPopup } from "./view.js";

const DEFAULT_SESSION = "work";

export async function mountPopup(doc: Document, deps: PopupDeps): Promise<void> {
  const root = doc.getElementById("app");
  if (!root) return;
  const lang = pickLanguage(deps.language);
  doc.documentElement.lang = lang;
  let state: PopupState = { kind: "loading" };
  // The HTML is built from escaped values only (view.ts); parsed rather than assigned to innerHTML.
  const draw = () => {
    const parsed = new DOMParser().parseFromString(renderPopup(state, lang), "text/html");
    root.replaceChildren(...parsed.body.childNodes);
  };
  draw();

  const tab = await deps.activeTab();
  const scope = tab ? scopeOf(tab.url) : null;
  if (!tab || !scope?.ok) {
    state = { kind: "unsupported" };
    return draw();
  }
  state = { kind: "ready", origin: scope.value.origin, session: (await deps.loadSession()) ?? DEFAULT_SESSION, ttlSeconds: TTL_CHOICES[0], includeLocalStorage: false, busy: false };
  draw();

  const update = (patch: Partial<Extract<PopupState, { kind: "ready" }>>) => {
    if (state.kind !== "ready") return;
    const { error: _e, done: _d, ...rest } = state;
    state = { ...rest, ...patch };
    draw();
  };

  root.addEventListener("input", (e) => {
    const target = e.target as HTMLInputElement;
    if (target.id === "session" && state.kind === "ready") state = { ...state, session: target.value.trim() };
  });
  root.addEventListener("click", async (e) => {
    const el = (e.target as HTMLElement).closest("[data-ttl], #ls, #submit");
    if (!el || state.kind !== "ready" || state.busy) return;
    if (el instanceof HTMLElement && el.dataset.ttl) return update({ ttlSeconds: Number(el.dataset.ttl) });
    if (el.id === "ls") return update({ includeLocalStorage: !state.includeLocalStorage });
    // The permission prompt must open inside this click.
    const granted = await deps.requestHostAccess(hostPatterns(scope.value)).catch(() => false);
    if (!granted) return update({ error: "no_permission" });
    update({ busy: true });
    const current = state;
    if (current.kind !== "ready") return;
    const result = await deps.handOff({ tabId: tab.id, session: current.session, ttlSeconds: current.ttlSeconds, includeLocalStorage: current.includeLocalStorage });
    if (result.ok) {
      await deps.saveSession(result.value.session);
      update({ busy: false, done: { cookies: result.value.cookies, session: result.value.session } });
    } else {
      update({ busy: false, error: result.error });
    }
  });
}
