// The app's commands (app/src/main.rs), as the window calls them. One port, so views are
// tested with a fake. Only `reveal` returns a value.

/// `auth` is the authentication method's name (`Authenticator::id`, 03 §3.10): `none` for now.
export interface Overview { vault_ready: boolean; sessions_open: number; requests_pending: number; drafts: number; auth: string }
export interface Item {
  name: string; kind: string; domain: string; status: string; source: string;
  description: string | null; fields: string[]; placeholders: string[];
  plain_values: Record<string, string>; origins: string[]; tags: string[];
  expired: boolean; tampered: boolean;
}
export interface Site { origin: string; state: string; item: string | null; checked_at: string }
export interface Session {
  id: string; name: string; state: string; cdp: boolean; description: string | null; sites: Site[];
  /// `chrome` or `ego-lite` while the session has a run, and whether others can drive it.
  browser: string | null; protection: "owned" | "shared" | null;
  /// Why the last run did not start.
  failure: string | null;
  /// From Rust (`DisplayState`): a run is under way, or only [복구…] brings it back.
  live: boolean; recoverable: boolean;
  /// Why a live session stopped substituting (`mask_probe`, `focus_moved`); [다시 허용] lifts it.
  locked?: string | null;
}
export interface Browsers { ego_lite: boolean }
export interface Request { id: string; agent: string; kind: string; name: string; origin: string | null; reason: string }
export interface Draft { name: string; kind: string; source: string; origins: string[] }
export interface Inbox { requests: Request[]; drafts: Draft[] }
/// `denied` is Rust's `AuditKind::is_denial`, the rule the denial notices use too (J-200).
export interface AuditEvent { at: string; agent: string; kind: string; denied: boolean; item: string | null; origin: string | null; result: string }
/// A field a form asks for (Rust `FormField`, one shape for the request form and the item form).
/// `secret` fields are password inputs, never echoed.
export interface FormField { name: string; secret: boolean }
export interface RequestForm {
  id: string; agent: string; kind: string; name: string; origin: string | null; reason: string;
  fields: FormField[]; free_fields: boolean; custom_fields: boolean;
}
export interface Handoff { session: string; session_name: string; request_id: string; origin: string | null }
/// Which change asks the person first (domain `SettingWeakening`, J-203).
export type Weakening =
  | { when: "set_to"; value: string }
  | { when: "above"; than: string }
  | { when: "any_change" }
  | { when: "never" };
export interface Setting { key: string; value: string; default: string; weakening: Weakening }
export interface Rule { agent: string; resource: string; target: string; effect: string }
/// A rule's agent or target that means every one (Rust `policy::ANY`, test/vocabulary.test.ts).
export const ANY = "*";
/// `generates` names the field the form can make up (`KindSpec::generated`), if any.
export interface KindForm { kind: string; domain: string; fields: FormField[]; free_fields: boolean; custom_fields: boolean; generates: string | null }
export interface NewItemForm { name: string; kind: string; origins: string[]; values: Record<string, string>; generate_password: boolean }
export interface ChangeForm { values: Record<string, string>; origins?: string[] }
/// How a site dragged onto a session ended; `words.ts` says it (J-202).
export type SiteHandOff =
  | { outcome: "handed_off"; cookies: number }
  | { outcome: "declined" | "no_permission" | "no_session" | "bad_site" | "timeout" | "failed" };
/// A change that may need a person: done, or the question to put to them.
export type Outcome = { outcome: "done" } | { outcome: "ask"; question: string };

export interface Api {
  overview(): Promise<Overview>;
  items(): Promise<Item[]>;
  reveal(item: string, field: string): Promise<string>;
  sessions(): Promise<Session[]>;
  inbox(): Promise<Inbox>;
  audit(limit: number): Promise<AuditEvent[]>;
  declineRequest(id: string): Promise<void>;
  confirmDraft(name: string): Promise<string>;
  discardDraft(name: string): Promise<void>;
  requestForm(id: string): Promise<RequestForm>;
  fulfillRequest(id: string, values: Record<string, string>): Promise<string>;
  handoffs(): Promise<Handoff[]>;
  resumeHandoff(session: string, request: string): Promise<void>;
  settings(): Promise<Setting[]>;
  setSetting(key: string, value: string, confirmed: boolean): Promise<Outcome>;
  policies(): Promise<Rule[]>;
  setPolicy(rule: Rule, confirmed: boolean): Promise<Outcome>;
  /// A rule is named by agent, resource and target; its effect does not pick it.
  removePolicy(rule: Omit<Rule, "effect">, confirmed: boolean): Promise<Outcome>;
  copyValue(item: string, field: string): Promise<void>;
  copyPlaceholder(text: string): Promise<void>;
  /// `browser` is `chrome` or `ego-lite`; none follows the `browser` setting.
  openSession(id: string, browser?: string): Promise<void>;
  removeSession(id: string): Promise<void>;
  browsers(): Promise<Browsers>;
  stopSession(id: string): Promise<void>;
  recoverSession(id: string, browserClosed: boolean): Promise<void>;
  /// Lifts a session's substitution lock (#190): the person's click, past the authenticator.
  unlockSession(id: string): Promise<void>;
  kinds(): Promise<KindForm[]>;
  addItem(form: NewItemForm): Promise<string>;
  editItem(name: string, form: ChangeForm, confirmed: boolean): Promise<Outcome>;
  removeItem(name: string): Promise<void>;
  renameItem(from: string, to: string): Promise<string>;
  requestSite(session: string, url: string): Promise<SiteHandOff>;
  openRequestWindow(id: string): Promise<void>;
  closeRequestWindow(): Promise<void>;
}

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

export function tauriApi(invoke: Invoke): Api {
  const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) as Promise<T>;
  return {
    overview: () => call("overview"),
    items: () => call("items"),
    reveal: (item, field) => call("reveal", { item, field }),
    sessions: () => call("sessions"),
    inbox: () => call("inbox"),
    audit: (limit) => call("audit", { limit }),
    declineRequest: (id) => call("decline_request", { id }),
    confirmDraft: (name) => call("confirm_draft", { name }),
    discardDraft: (name) => call("discard_draft", { name }),
    requestForm: (id) => call("request_form", { id }),
    fulfillRequest: (id, values) => call("fulfill_request", { id, values }),
    handoffs: () => call("handoffs"),
    resumeHandoff: (session, request) => call("resume_handoff", { session, request }),
    settings: () => call("settings"),
    setSetting: (key, value, confirmed) => call("set_setting", { key, value, confirmed }),
    policies: () => call("policies"),
    setPolicy: (rule, confirmed) => call("set_policy", { rule, confirmed }),
    removePolicy: (r, confirmed) => call("remove_policy", { agent: r.agent, resource: r.resource, target: r.target, confirmed }),
    copyValue: (item, field) => call("copy_value", { item, field }),
    copyPlaceholder: (text) => call("copy_placeholder", { text }),
    openSession: (id, browser) => call("open_session", { id, browser: browser ?? null }),
    removeSession: (id) => call("remove_session", { id }),
    browsers: () => call("browsers"),
    stopSession: (id) => call("stop_session", { id }),
    recoverSession: (id, browserClosed) => call("recover_session", { id, browserClosed }),
    unlockSession: (id) => call("unlock_session", { id }),
    kinds: () => call("kinds"),
    addItem: (form) => call("add_item", { form }),
    editItem: (name, form, confirmed) => call("edit_item", { name, form, confirmed }),
    removeItem: (name) => call("remove_item", { name }),
    renameItem: (from, to) => call("rename_item", { from, to }),
    requestSite: (session, url) => call("request_site", { session, url }),
    openRequestWindow: (id) => call("open_request_window", { id }),
    closeRequestWindow: () => call("close_request_window"),
  };
}
