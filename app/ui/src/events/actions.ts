// What a click on a `data-action` button does: one handler per action name (#182). A name no
// handler has does nothing. The handlers that carry a value (reveal, copy) are not here: main.ts
// keeps them as they were and adds them to this table.

import {
  asking, edit, editNew, go, isView, removing, removingSession, search, select, openForm,
} from "../state.js";
import { customRow } from "../views/editor.js";
import { ITEM_REMOVAL, SESSION_REMOVAL, undoTimer } from "./undo.js";
import { type Win, change, finishRequest, load, matches, runCommand } from "./window.js";

/// `data` is the button's `data-*` attributes.
export type Handler = (data: DOMStringMap, button: HTMLElement) => void;
export type Actions = Record<string, Handler>;

/// Runs the handler `button`'s `data-action` names, if there is one.
export function dispatch(actions: Actions, button: HTMLElement): void {
  const { action } = button.dataset;
  if (action === undefined || !Object.hasOwn(actions, action)) return;
  actions[action]?.(button.dataset, button);
}

/// Every action but those that carry a value.
export function windowActions(w: Win): Actions {
  const { api, guard } = w;
  const reload = () => load(w);
  const items = undoTimer(w, ITEM_REMOVAL);
  const sessions = undoTimer(w, SESSION_REMOVAL);
  return {
    go: ({ view }) => {
      if (view && isView(view)) w.set(go(w.state, view));
    },
    select: ({ name }) => {
      if (name) w.set(select(w.state, name));
    },
    "copy-placeholder": ({ text }) => {
      if (text) void guard(() => api.copyPlaceholder(text));
    },
    "open-session": ({ id, browser }) => {
      if (id) void guard(() => api.openSession(id, browser).then(reload));
    },
    "remove-session": ({ id, name }) => {
      if (id) sessions.start((s) => removingSession(s, id, name ?? id));
    },
    "undo-remove-session": () => sessions.undo(),
    "stop-session": ({ id }) => {
      if (id) void guard(() => api.stopSession(id).then(reload));
    },
    "unlock-session": ({ id }) => {
      if (id) void guard(() => api.unlockSession(id).then(reload));
    },
    recover: ({ id }) => {
      if (!id) return;
      const box = w.root.querySelector<HTMLInputElement>(`[data-closed="${CSS.escape(id)}"]`);
      void guard(() => api.recoverSession(id, box?.checked ?? false).then(reload));
    },
    "open-form": ({ id }) => {
      if (id) void guard(async () => w.set(openForm(w.state, await api.requestForm(id))));
    },
    "close-window": () => void guard(() => api.closeRequestWindow()),
    decline: ({ id }) => {
      if (id) void guard(() => api.declineRequest(id).then(() => finishRequest(w)));
    },
    "open-window": ({ id }) => {
      if (id) void guard(() => api.openRequestWindow(id));
    },
    resume: ({ session, id }) => {
      if (session && id) void guard(() => api.resumeHandoff(session, id).then(reload));
    },
    confirm: ({ name }) => {
      if (name) void guard(() => api.confirmDraft(name).then(reload));
    },
    discard: ({ name }) => {
      if (name) void guard(() => api.discardDraft(name).then(reload));
    },
    "remove-rule": ({ agent, resource, target }) => {
      if (agent && resource && target) void change(w, (confirmed) => api.removePolicy({ agent, resource, target }, confirmed));
    },
    "new-item": () => w.set(editNew(w.state)),
    "edit-item": ({ name }) => {
      if (name) w.set(edit(w.state, { mode: "edit", name }));
    },
    "rename-item": ({ name }) => {
      if (name) w.set(edit(w.state, { mode: "rename", name }));
    },
    "close-editor": () => w.set(edit(w.state, null)),
    "add-custom-field": (_data, button) => {
      // Added in place, not by re-rendering: what is typed in the form stays.
      const rows = button.closest("fieldset")?.querySelector(".custom-rows");
      rows?.insertAdjacentHTML("beforeend", customRow(rows.children.length));
    },
    "remove-template": ({ name }) => {
      if (name) void guard(() => api.removeTemplate(name).then(reload));
    },
    "remove-item": ({ name }) => {
      if (name) items.start((s) => removing(s, name));
    },
    "undo-remove": () => items.undo(),
    run: ({ index }) => {
      const picked = index === undefined ? undefined : matches(w)[Number(index)];
      if (picked) runCommand(w, picked.run);
    },
    "set-setting": ({ key, value }) => {
      if (key && value !== undefined) void change(w, (confirmed) => api.setSetting(key, value, confirmed));
    },
    "filter-kind": ({ kind }) => w.set(search(w.state, { kind: kind || null })),
    "filter-drafts": () => w.set(search(w.state, { drafts: !w.state.filter.drafts })),
    "ask-cancel": () => w.set(asking(w.state, null)),
    "ask-confirm": () => {
      if (!w.state.ask) return;
      const { retry } = w.state.ask;
      w.set(asking(w.state, null));
      void guard(retry);
    },
  };
}
