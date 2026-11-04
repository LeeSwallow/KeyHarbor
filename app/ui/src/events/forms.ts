// What submitting a form does: one handler per kind of form, told apart by its data attribute
// (#182). The secure input form's handler is not here: main.ts keeps it as it was.

import { ANY } from "../api.js";
import { edit, select, warned } from "../state.js";
import { editorValues } from "../views/editor.js";
import { joinFolders, parseFolders } from "../views/settings.js";
import { type Win, change, load } from "./window.js";

export type FormKind = "request" | "new-item" | "edit-item" | "rename-item" | "setting" | "rule";
export type FormHandler = (el: HTMLFormElement, data: FormData) => void;
export type Forms = Record<FormKind, FormHandler>;

/// Which form `el` is; the secure input form (`data-form`) first.
export function formKind(el: HTMLFormElement): FormKind | null {
  const d = el.dataset;
  if (d.form) return "request";
  if (d.editor === "new") return "new-item";
  if (d.editor === "edit") return "edit-item";
  if (d.editor === "rename") return "rename-item";
  if (d.setting) return "setting";
  if (d.rule !== undefined) return "rule";
  return null;
}

/// A typed text field, or "" when the form has none by that name.
const text = (data: FormData, name: string): string => {
  const value = data.get(name);
  return typeof value === "string" ? value : "";
};

/// Every form but the secure input form.
export function windowForms(w: Win): Omit<Forms, "request"> {
  const { api, guard } = w;
  const selectAfterLoad = (name: string) => load(w).then(() => w.set(select(w.state, name)));
  return {
    "new-item": (el, data) => {
      // A form that names no kind is not sent: no item is stored as "undefined" (#182).
      const kind = text(data, "kind") || el.dataset.kind;
      if (!kind) return;
      const { values, origins } = editorValues(data);
      const item = { name: text(data, "name"), kind, origins, values, generate_password: data.get("generate") === "on" };
      el.reset();
      void guard(() => api.addItem(item).then((saved) => selectAfterLoad(saved.name).then(() => w.set(warned(w.state, saved.warnings)))));
    },
    "edit-item": (el, data) => {
      const name = el.dataset.name;
      if (!name) return;
      const { values, origins } = editorValues(data);
      el.reset();
      // The warnings land with the answer that stored the change, the confirmed resend too.
      const send = (confirmed: boolean) =>
        api.editItem(name, { values, origins }, confirmed).then((edited) => (w.set(warned(w.state, edited.warnings)), edited));
      void change(w, send).then(() => w.set(edit(w.state, null)));
    },
    "rename-item": (el, data) => {
      const from = el.dataset.name;
      if (!from) return;
      const to = text(data, "to").trim();
      void guard(() => api.renameItem(from, to).then(selectAfterLoad));
    },
    setting: (el, data) => {
      const key = el.dataset.setting;
      if (!key) return;
      const typed = text(data, "value").trim();
      // A list setting (upload_dirs) adds the typed entry to what it has.
      const value = el.dataset.append !== undefined ? joinFolders([...parseFolders(el.dataset.append), typed]) : typed;
      void change(w, (confirmed) => api.setSetting(key, value, confirmed));
    },
    rule: (_el, data) => {
      const rule = {
        agent: text(data, "agent") || ANY,
        resource: text(data, "resource"),
        target: text(data, "target") || ANY,
        effect: text(data, "effect"),
      };
      void change(w, (confirmed) => api.setPolicy(rule, confirmed));
    },
  };
}
