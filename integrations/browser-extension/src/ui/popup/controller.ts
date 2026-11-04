// The popup's state and one handler per flow. No DOM here (popup.ts wires the page to it), so
// the flows are tested without a browser. Every call to the background is caught: a rejected
// promise draws an error and releases `busy`, never a stuck button or a stuck loading screen.

import { TTL_CHOICES } from "../../domain/handoff.js";
import { err } from "../../domain/result.js";
import { type SiteScope, hostPatterns, scopeOf } from "../../domain/scope.js";
import type { PopupDeps } from "./deps.js";
import type { PopupState } from "./view.js";

const DEFAULT_SESSION = "work";

type Ready = Extract<PopupState, { kind: "ready" }>;
type ReadyPatch = Partial<Omit<Ready, "kind">>;

/** A change to one of the hand-off options. */
export type Toggle = { kind: "ttl"; seconds: number } | { kind: "localStorage" };

export interface PopupController {
  /** Reads the tab and the last session, then draws the popup. Never rejects. */
  start(): Promise<void>;
  onSession(value: string): void;
  onToggle(toggle: Toggle): void;
  onFill(item: string): Promise<void>;
  onSaveLogin(): Promise<void>;
  onHandOff(): Promise<void>;
}

/** The background could not be reached: the message never got an answer. */
const unreachable = () => err("host_unavailable");

export function makePopupController(deps: PopupDeps, draw: (state: PopupState) => void): PopupController {
  let state: PopupState = { kind: "loading" };
  let tabId = 0;
  let scope: SiteScope | null = null;

  const set = (next: PopupState) => {
    state = next;
    draw(state);
  };
  /** Applies a patch to the ready popup; the last error and outcomes go with any change. */
  const update = (patch: ReadyPatch) => {
    if (state.kind !== "ready") return;
    const { error: _e, done: _d, login: _l, ...rest } = state;
    set({ ...rest, ...patch });
  };
  /** The ready popup when nothing is running, else null: one flow at a time. */
  const idle = (): Ready | null => (state.kind === "ready" && !state.busy ? state : null);

  return {
    async start() {
      set({ kind: "loading" });
      const tab = await deps.activeTab().catch(() => null);
      const parsed = tab ? scopeOf(tab.url) : null;
      if (!tab || !parsed?.ok) return set({ kind: "unsupported" });
      tabId = tab.id;
      scope = parsed.value;
      const session = (await deps.loadSession().catch(() => null)) ?? DEFAULT_SESSION;
      set({ kind: "ready", origin: scope.origin, session, ttlSeconds: TTL_CHOICES[0], includeLocalStorage: false, busy: false });
      void deps.listLogins(tabId).then(
        (r) => update({ logins: r.ok ? r.value : null }),
        () => update({ logins: null }),
      );
    },

    onSession(value) {
      // Kept as typed, without a redraw; parseSessionName trims, as Rust does.
      if (state.kind === "ready") state = { ...state, session: value };
    },

    onToggle(toggle) {
      const s = idle();
      if (!s) return;
      update(toggle.kind === "ttl" ? { ttlSeconds: toggle.seconds } : { includeLocalStorage: !s.includeLocalStorage });
    },

    async onFill(item) {
      if (!idle()) return;
      update({ busy: true });
      const result = await deps.fillLogin(tabId, item).catch(unreachable);
      update(result.ok ? { busy: false, login: { kind: "filled", item } } : { busy: false, error: result.error });
    },

    async onSaveLogin() {
      if (!idle()) return;
      update({ busy: true });
      const result = await deps.saveLogin(tabId).catch(unreachable);
      update(result.ok ? { busy: false, login: result.value } : { busy: false, error: result.error });
    },

    async onHandOff() {
      if (!idle() || !scope) return;
      // The permission prompt must open inside the click, so it comes before anything awaits.
      const granted = await deps.requestHostAccess(hostPatterns(scope)).catch(() => false);
      const s = idle();
      if (!s) return;
      if (!granted) return update({ error: "no_permission" });
      update({ busy: true });
      const result = await deps.handOff({ tabId, session: s.session, ttlSeconds: s.ttlSeconds, includeLocalStorage: s.includeLocalStorage }).catch(unreachable);
      if (!result.ok) return update({ busy: false, error: result.error });
      // Remembering the session is a convenience: the hand-off is done either way.
      await deps.saveSession(result.value.session).catch(() => undefined);
      update({ busy: false, done: { cookies: result.value.cookies, session: result.value.session } });
    },
  };
}
