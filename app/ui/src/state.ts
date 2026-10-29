// The window's state and its transitions, as pure functions. A revealed value lives only in
// `revealed` and every move away (another screen, another item, hiding) drops it (09 §9.12,
// Review Focus 1).

import type { AuditEvent, Inbox, Item, Overview, Session } from "./api.js";

export type View = "home" | "inbox" | "sessions" | "items" | "audit";

export interface Revealed { item: string; field: string; value: string }

export interface State {
  view: View;
  overview: Overview | null;
  inbox: Inbox;
  sessions: Session[];
  items: Item[];
  audit: AuditEvent[];
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
  selected: null,
  revealed: null,
  error: null,
};

export const go = (s: State, view: View): State => ({ ...s, view, selected: null, revealed: null, error: null });
export const select = (s: State, name: string): State => ({ ...s, selected: name, revealed: null });
export const show = (s: State, revealed: Revealed): State =>
  s.selected === revealed.item ? { ...s, revealed } : s;
export const hide = (s: State): State => ({ ...s, revealed: null });
export const fail = (s: State, error: string): State => ({ ...s, error });
