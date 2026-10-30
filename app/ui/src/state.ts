// The window's state and its transitions, as pure functions. A revealed value lives only in
// `revealed` and every move away (another screen, another item, hiding) drops it (09 §9.12,
// Review Focus 1).

import type { AuditEvent, Handoff, Inbox, Item, KindForm, Overview, RequestForm, Rule, Session, Setting } from "./api.js";

export type View = "home" | "inbox" | "sessions" | "items" | "audit" | "policies" | "settings";

/// The screens in the side bar and the palette, in order, with the design-system icon each
/// shows in its tab, its title and the palette.
export const SCREENS: [View, string, string][] = [
  ["home", "홈", "house"],
  ["inbox", "확인 대기", "inbox"],
  ["sessions", "세션", "app-window"],
  ["items", "항목", "key-round"],
  ["audit", "사용 기록", "scroll-text"],
  ["policies", "에이전트와 정책", "shield-check"],
  ["settings", "설정", "settings"],
];

/// The item list's search (09 §9.4, ⌘F): text in names, sites or tags, one kind, drafts only.
export interface Filter { text: string; kind: string | null; drafts: boolean }
const NO_FILTER: Filter = { text: "", kind: null, drafts: false };

/// The command palette, open, with what was typed.
export interface Palette { query: string }

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
  palette: Palette | null;
  filter: Filter;
  /// The secure input window (09 §9.4.5): only the form for one request, nothing else.
  alone: boolean;
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
  palette: null,
  filter: NO_FILTER,
  alone: false,
  selected: null,
  revealed: null,
  error: null,
};

export const go = (s: State, view: View): State =>
  ({ ...s, view, selected: null, revealed: null, form: null, ask: null, editor: null, error: null, filter: NO_FILTER });
export const search = (s: State, patch: Partial<Filter>): State => ({ ...s, filter: { ...s.filter, ...patch } });

/// The items `f` lets through, in their order.
export function filterItems(items: Item[], f: Filter): Item[] {
  const want = f.text.trim().toLowerCase();
  const hit = (i: Item) => [i.name, ...i.origins, ...i.tags].some((t) => t.toLowerCase().includes(want));
  return items.filter((i) => (!f.kind || i.kind === f.kind) && (!f.drafts || i.status === "draft") && (!want || hit(i)));
}
export const openForm = (s: State, form: RequestForm): State => ({ ...s, view: "inbox", form, revealed: null });
export const alone = (s: State, form: RequestForm | null): State => ({ ...s, alone: true, form });
export const closeForm = (s: State): State => ({ ...s, form: null });
export const asking = (s: State, ask: Ask | null): State => ({ ...s, ask });
export const select = (s: State, name: string): State => ({ ...s, selected: name, revealed: null, editor: null });
export const edit = (s: State, editor: Editor | null): State => ({ ...s, editor, revealed: null });
export const show = (s: State, revealed: Revealed): State =>
  s.selected === revealed.item ? { ...s, revealed } : s;
export const hide = (s: State): State => ({ ...s, revealed: null });
export const openPalette = (s: State): State => ({ ...s, palette: { query: "" } });
export const query = (s: State, typed: string): State => (s.palette ? { ...s, palette: { query: typed } } : s);
export const closePalette = (s: State): State => ({ ...s, palette: null });
export const fail = (s: State, error: string): State => ({ ...s, error });
