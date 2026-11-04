// Wires the window: loads data through the Tauri port, keeps one State, re-renders on change.
// A revealed value is hidden again after 30 seconds (09 §9.9's clipboard limit, applied to
// the screen too). A change that weakens a protection comes back as a question; only the
// dialog's confirm button sends it again, confirmed. What a click or a form does lives in the
// tables under events/ (#182); what carries a value stays here.

import { type Api, tauriApi } from "./api.js";
import { type State, alone, fail, hide, initial, show } from "./state.js";
import { render } from "./view.js";
import { formValues } from "./views/form.js";
import { type Actions, windowActions } from "./events/actions.js";
import { type FormHandler, windowForms } from "./events/forms.js";
import { listenClicks, listenDrops, listenInput, listenKeys, listenPicks, listenSubmits } from "./events/listeners.js";
import { type Win, finishRequest, load, refocus } from "./events/window.js";

const HIDE_AFTER_MS = 30_000;

/// `request` set: this is the secure input window (09 §9.4.5) for that request, showing its form only.
export function mount(root: HTMLElement, api: Api, request: string | null = null): { state: () => State } {
  let state = initial;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  const set = (next: State) => {
    state = next;
    root.innerHTML = render(state);
    refocus(root, w.typing);
  };
  const guard = async (work: () => Promise<unknown>) => {
    try {
      await work();
    } catch (error) {
      set(fail(state, String(error)));
    }
  };
  const w: Win = { root, api, get state() { return state; }, set, guard, typing: false };
  const answered = () => finishRequest(w);
  // What carries a value (09 §9.12) stays here as it was; #182 only moved it into the tables.
  const valueActions: Actions = {
    reveal: ({ name, field }) => {
      if (!name || !field) return;
      void guard(async () => {
        const value = await api.reveal(name, field);
        set(show(state, { item: name, field, value }));
        clearTimeout(hideTimer);
        hideTimer = setTimeout(() => set(hide(state)), HIDE_AFTER_MS);
      });
    },
    hide: () => set(hide(state)),
    copy: ({ name, field }) => {
      if (name && field) void guard(() => api.copyValue(name, field));
    },
  };
  const requestForm: FormHandler = (el, data) => {
    if (el.dataset.form) {
      const id = el.dataset.form;
      const values = formValues(data);
      el.reset();
      void guard(() => api.fulfillRequest(id, values).then(answered));
    }
  };
  listenClicks(w, { ...windowActions(w), ...valueActions });
  listenDrops(w);
  listenInput(w);
  listenKeys(w);
  listenPicks(w);
  listenSubmits(w, { ...windowForms(w), request: requestForm });
  set(state);
  if (request) {
    void api.requestForm(request).then((f) => set(alone(state, f)), () => set(alone(state, null)));
  } else {
    void load(w);
    // Asking ego lite takes a few seconds and starts a process: once per window, not per reload.
    // No answer is not an error to show: `browsers` stays as it starts (no ego lite), so the
    // session buttons offer Chrome only.
    void api.browsers().then((browsers) => set({ ...state, browsers }), () => undefined);
  }
  return { state: () => state };
}

declare global {
  interface Window {
    __TAURI__?: { core: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> } };
  }
}

const root = typeof document !== "undefined" ? document.getElementById("app") : null;
if (root && window.__TAURI__) {
  mount(root, tauriApi(window.__TAURI__.core.invoke), new URLSearchParams(location.search).get("request"));
}
