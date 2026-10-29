// Wires the window: loads data through the Tauri port, keeps one State, re-renders on change.
// A revealed value is hidden again after 30 seconds (09 §9.9's clipboard limit, applied to
// the screen too).

import { type Api, tauriApi } from "./api.js";
import { type State, type View, fail, go, hide, initial, select, show } from "./state.js";
import { render } from "./view.js";

const HIDE_AFTER_MS = 30_000;

export function mount(root: HTMLElement, api: Api): { state: () => State } {
  let state = initial;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  const set = (next: State) => {
    state = next;
    root.innerHTML = render(state);
  };
  const guard = async (work: () => Promise<void>) => {
    try {
      await work();
    } catch (error) {
      set(fail(state, String(error)));
    }
  };
  const load = () =>
    guard(async () => {
      const [overview, inbox, sessions, items, audit] = await Promise.all([
        api.overview(), api.inbox(), api.sessions(), api.items(), api.audit(50),
      ]);
      set({ ...state, overview, inbox, sessions, items, audit });
    });
  root.addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLElement>("[data-action]");
    if (!button) return;
    const { action, view, name, field, id } = button.dataset;
    if (action === "go" && view) set(go(state, view as View));
    if (action === "select" && name) set(select(state, name));
    if (action === "hide") set(hide(state));
    if (action === "reveal" && name && field) {
      void guard(async () => {
        const value = await api.reveal(name, field);
        set(show(state, { item: name, field, value }));
        clearTimeout(hideTimer);
        hideTimer = setTimeout(() => set(hide(state)), HIDE_AFTER_MS);
      });
    }
    if (action === "decline" && id) void guard(() => api.declineRequest(id).then(load));
    if (action === "confirm" && name) void guard(() => api.confirmDraft(name).then(load));
    if (action === "discard" && name) void guard(() => api.discardDraft(name).then(load));
  });
  set(state);
  void load();
  return { state: () => state };
}

declare global {
  interface Window {
    __TAURI__?: { core: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> } };
  }
}

const root = typeof document !== "undefined" ? document.getElementById("app") : null;
if (root && window.__TAURI__) mount(root, tauriApi(window.__TAURI__.core.invoke));
