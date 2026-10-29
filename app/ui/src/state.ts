// The window's state and its transitions, as pure functions. A revealed value lives only in
// `revealed` and every move away (another screen, another item, hiding) drops it (09 §9.12,
// Review Focus 1).

import type { AuditEvent, Handoff, Inbox, Item, KindForm, Overview, RequestForm, Rule, Session, Setting } from "./api.js";

export type View = "home" | "inbox" | "sessions" | "items" | "audit" | "policies" | "settings";

/// A question a change is waiting on, and the change to send again once confirmed.
export interface Ask { question: string; retry: () => Promise<unknown>; title?: string }

/// The item form open in the detail pane: a new item of a kind, or a change to one.
export type Editor = { mode: "new"; kind: string } | { mode: "edit"; name: string };

export interface Revealed { item: string; field: string; value: string }

export interface State {
  view: View;
  overview: Overview | null;
  inbox: Inbox;
  sessions: Session[];
  items: Item[];
  audit: AuditEvent[];
  handoffs: Handoff[];
  settings: Setting[];
  policies: Rule[];
  /// The secure input form being filled; its values live only in the form's inputs.
  form: RequestForm | null;
  kinds: KindForm[];
  editor: Editor | null;
  ask: Ask | null;
  selected: string | null;
  revealed: Revealed | null;
  error: string | null;
}

export const initial: State = {
  view: "home",
  overview: null,
  inbox: { requests: [], drafts: [] },
  sessions: [],
  items: [],
  audit: [],
  handoffs: [],
  settings: [],
  policies: [],
  form: null,
  kinds: [],
  editor: null,
  ask: null,
  selected: null,
  revealed: null,
  error: null,
};

export const go = (s: State, view: View): State =>
  ({ ...s, view, selected: null, revealed: null, form: null, ask: null, editor: null, error: null });
export const openForm = (s: State, form: RequestForm): State => ({ ...s, view: "inbox", form, revealed: null });
export const closeForm = (s: State): State => ({ ...s, form: null });
export const asking = (s: State, ask: Ask | null): State => ({ ...s, ask });
export const select = (s: State, name: string): State => ({ ...s, selected: name, revealed: null, editor: null });
export const edit = (s: State, editor: Editor | null): State => ({ ...s, editor, revealed: null });
export const show = (s: State, revealed: Revealed): State =>
  s.selected === revealed.item ? { ...s, revealed } : s;
export const hide = (s: State): State => ({ ...s, revealed: null });
export const fail = (s: State, error: string): State => ({ ...s, error });
