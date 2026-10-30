// Wires the window: loads data through the Tauri port, keeps one State, re-renders on change.
// A revealed value is hidden again after 30 seconds (09 §9.9's clipboard limit, applied to
// the screen too). A change that weakens a protection comes back as a question; only the
// dialog's confirm button sends it again, confirmed.

import { type Api, type Outcome, tauriApi } from "./api.js";
import {
  type State, type View, alone, asking, closeForm, closePalette, edit, fail, go, hide, initial, openForm, openPalette,
  query, search, select, show,
} from "./state.js";
import { render } from "./view.js";
import { editorValues } from "./views/editor.js";
import { formValues } from "./views/form.js";
import { type Run, paletteCommands } from "./views/palette.js";

const HIDE_AFTER_MS = 30_000;

/// `request` set: this is the secure input window (09 §9.4.5) for that request, showing its form only.
export function mount(root: HTMLElement, api: Api, request: string | null = null): { state: () => State } {
  let state = initial;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  /// Whether the person is typing in the item search, so a re-render gives it focus back.
  let typing = false;
  const set = (next: State) => {
    state = next;
    root.innerHTML = render(state);
    // Re-rendering replaces the palette and search inputs; keep typing where it was.
    const input = root.querySelector<HTMLInputElement>("[data-palette]") ?? (typing ? root.querySelector<HTMLInputElement>("[data-search]") : null);
    if (input) {
      input.focus();
      input.setSelectionRange(input.value.length, input.value.length);
    }
  };
  const run = (r: Run) => {
    const closed = closePalette(state);
    if (r.kind === "go") set(go(closed, r.view));
    if (r.kind === "select") set(select(go(closed, "items"), r.name));
    if (r.kind === "new-item") set(edit(go(closed, "items"), { mode: "new", kind: state.kinds[0]?.kind ?? "login" }));
  };
  const matches = () => (state.palette ? paletteCommands(state, state.palette.query) : []);
  const guard = async (work: () => Promise<unknown>) => {
    try {
      await work();
    } catch (error) {
      set(fail(state, String(error)));
    }
  };
  const load = () =>
    guard(async () => {
      const [overview, inbox, sessions, items, audit, handoffs, settings, policies, kinds] = await Promise.all([
        api.overview(), api.inbox(), api.sessions(), api.items(), api.audit(50),
        api.handoffs(), api.settings(), api.policies(), api.kinds(),
      ]);
      set({ ...state, overview, inbox, sessions, items, audit, handoffs, settings, policies, kinds });
    });
  /// After a request is answered: the secure input window closes, the main window reloads.
  const answered = () => (state.alone ? api.closeRequestWindow() : (set(closeForm(state)), load()));
  /// Sends a change; if it needs a person, asks first and resends it confirmed on "확인".
  const change = (send: (confirmed: boolean) => Promise<Outcome>) =>
    guard(async () => {
      const outcome = await send(false);
      if (outcome.outcome === "ask") {
        set(asking(state, { question: outcome.question, retry: () => send(true).then(load) }));
      } else {
        await load();
      }
    });

  root.addEventListener("click", (event) => {
    typing = false;
    const button = (event.target as HTMLElement).closest<HTMLElement>("[data-action]");
    if (!button) return;
    const { action, view, name, field, id, session, agent, resource, target } = button.dataset;
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
    const { text } = button.dataset;
    if (action === "copy" && name && field) void guard(() => api.copyValue(name, field));
    if (action === "copy-placeholder" && text) void guard(() => api.copyPlaceholder(text));
    if (action === "open-session" && id) void guard(() => api.openSession(id).then(load));
    if (action === "stop-session" && id) void guard(() => api.stopSession(id).then(load));
    if (action === "recover" && id) {
      const box = root.querySelector<HTMLInputElement>(`[data-closed="${CSS.escape(id)}"]`);
      void guard(() => api.recoverSession(id, box?.checked ?? false).then(load));
    }
    if (action === "open-form" && id) void guard(async () => set(openForm(state, await api.requestForm(id))));
    if (action === "decline" && id) void guard(() => api.declineRequest(id).then(answered));
    if (action === "open-window" && id) void guard(() => api.openRequestWindow(id));
    if (action === "resume" && session && id) void guard(() => api.resumeHandoff(session, id).then(load));
    if (action === "confirm" && name) void guard(() => api.confirmDraft(name).then(load));
    if (action === "discard" && name) void guard(() => api.discardDraft(name).then(load));
    if (action === "remove-rule" && agent && resource && target) {
      void change((confirmed) => api.removePolicy({ agent, resource, target, effect: "" }, confirmed));
    }
    if (action === "new-item") set(edit(state, { mode: "new", kind: state.kinds[0]?.kind ?? "login" }));
    if (action === "edit-item" && name) set(edit(state, { mode: "edit", name }));
    if (action === "rename-item" && name) set(edit(state, { mode: "rename", name }));
    if (action === "close-editor") set(edit(state, null));
    if (action === "remove-item" && name) {
      set(asking(state, {
        title: "항목을 지웁니다",
        question: `${name}을(를) 지웁니다. 되돌릴 수 없습니다.`,
        retry: () => api.removeItem(name).then(() => set(go(state, "items"))).then(load),
      }));
    }
    const { index } = button.dataset;
    if (action === "run" && index !== undefined) {
      const picked = matches()[Number(index)];
      if (picked) run(picked.run);
    }
    const { key, value } = button.dataset;
    if (action === "set-setting" && key && value !== undefined) {
      void change((confirmed) => api.setSetting(key, value, confirmed));
    }
    if (action === "filter-kind") set(search(state, { kind: button.dataset.kind || null }));
    if (action === "filter-drafts") set(search(state, { drafts: !state.filter.drafts }));
    if (action === "ask-cancel") set(asking(state, null));
    if (action === "ask-confirm" && state.ask) {
      const { retry } = state.ask;
      set(asking(state, null));
      void guard(retry);
    }
  });

  root.addEventListener("input", (event) => {
    const input = event.target as HTMLInputElement;
    if (input.dataset.palette !== undefined) set(query(state, input.value));
    if (input.dataset.search !== undefined) {
      typing = true;
      set(search(state, { text: input.value }));
    }
  });

  // 09 U6: ⌘K opens and closes the palette, ⌘N starts a new item, Esc closes what is open.
  root.ownerDocument.addEventListener("keydown", (event) => {
    const command = event.metaKey || event.ctrlKey;
    if (command && event.key === "k") {
      event.preventDefault();
      set(state.palette ? closePalette(state) : openPalette(state));
    } else if (command && event.key === "f") {
      event.preventDefault();
      if (state.view !== "items") set(go(state, "items"));
      typing = true;
      root.querySelector<HTMLInputElement>("[data-search]")?.focus();
    } else if (command && event.key === "n") {
      event.preventDefault();
      run({ kind: "new-item" });
    } else if (event.key === "Escape" && state.palette) {
      set(closePalette(state));
    } else if (event.key === "Enter" && state.palette) {
      event.preventDefault();
      const first = matches()[0];
      if (first) run(first.run);
    }
  });

  root.addEventListener("change", (event) => {
    const picker = event.target as HTMLSelectElement;
    if (picker.dataset.action === "pick-kind") set(edit(state, { mode: "new", kind: picker.value }));
  });

  root.addEventListener("submit", (event) => {
    event.preventDefault();
    const el = event.target as HTMLFormElement;
    const data = new FormData(el);
    if (el.dataset.form) {
      const id = el.dataset.form;
      const values = formValues(data);
      el.reset();
      void guard(() => api.fulfillRequest(id, values).then(answered));
    } else if (el.dataset.editor === "new") {
      const { values, origins } = editorValues(data);
      const item = {
        name: String(data.get("name") ?? ""), kind: String(data.get("kind") ?? el.dataset.kind),
        origins, values, generate_password: data.get("generate") === "on",
      };
      el.reset();
      void guard(() => api.addItem(item).then((name) => load().then(() => set(select(state, name)))));
    } else if (el.dataset.editor === "edit" && el.dataset.name) {
      const name = el.dataset.name;
      const { values, origins } = editorValues(data);
      el.reset();
      void change((confirmed) => api.editItem(name, { values, origins }, confirmed)).then(() => set(edit(state, null)));
    } else if (el.dataset.editor === "rename" && el.dataset.name) {
      const from = el.dataset.name;
      const to = String(data.get("to") ?? "").trim();
      void guard(() => api.renameItem(from, to).then((name) => load().then(() => set(select(state, name)))));
    } else if (el.dataset.setting) {
      const key = el.dataset.setting;
      const value = String(data.get("value") ?? "");
      void change((confirmed) => api.setSetting(key, value, confirmed));
    } else if (el.dataset.rule !== undefined) {
      const rule = {
        agent: String(data.get("agent") || "*"),
        resource: String(data.get("resource")),
        target: String(data.get("target") || "*"),
        effect: String(data.get("effect")),
      };
      void change((confirmed) => api.setPolicy(rule, confirmed));
    }
  });
  set(state);
  if (request) {
    void api.requestForm(request).then((f) => set(alone(state, f)), () => set(alone(state, null)));
  } else {
    void load();
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
