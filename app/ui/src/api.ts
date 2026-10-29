// The app's commands (app/src/main.rs), as the window calls them. One port, so views are
// tested with a fake. Only `reveal` returns a value.

export interface Overview { vault_ready: boolean; sessions_open: number; requests_pending: number; drafts: number }
export interface Item {
  name: string; kind: string; domain: string; status: string; source: string;
  description: string | null; fields: string[]; placeholders: string[];
  plain_values: Record<string, string>; origins: string[]; tags: string[];
  expired: boolean; tampered: boolean;
}
export interface Site { origin: string; state: string; item: string | null; checked_at: string }
export interface Session { id: string; name: string; state: string; cdp: boolean; description: string | null; sites: Site[] }
export interface Request { id: string; agent: string; kind: string; name: string; origin: string | null; reason: string }
export interface Draft { name: string; kind: string; source: string; origins: string[] }
export interface Inbox { requests: Request[]; drafts: Draft[] }
export interface AuditEvent { at: string; agent: string; kind: string; item: string | null; origin: string | null; result: string }
export interface FormField { name: string; secret: boolean }
export interface RequestForm {
  id: string; agent: string; kind: string; name: string; origin: string | null; reason: string;
  fields: FormField[]; free_fields: boolean;
}
export interface Handoff { session: string; session_name: string; request_id: string; origin: string | null }
export interface Setting { key: string; value: string; default: string }
export interface Rule { agent: string; resource: string; target: string; effect: string }
/// A change that may need a person: done, or the question to put to them.
export interface KindForm { kind: string; domain: string; fields: [string, boolean][]; free_fields: boolean }
export interface NewItemForm { name: string; kind: string; origins: string[]; values: Record<string, string>; generate_password: boolean }
export interface ChangeForm { values: Record<string, string>; origins?: string[] }
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
  removePolicy(rule: Rule, confirmed: boolean): Promise<Outcome>;
  copyValue(item: string, field: string): Promise<void>;
  copyPlaceholder(text: string): Promise<void>;
  openSession(id: string): Promise<void>;
  stopSession(id: string): Promise<void>;
  recoverSession(id: string, browserClosed: boolean): Promise<void>;
  kinds(): Promise<KindForm[]>;
  addItem(form: NewItemForm): Promise<string>;
  editItem(name: string, form: ChangeForm, confirmed: boolean): Promise<Outcome>;
  removeItem(name: string): Promise<void>;
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
    setPolicy: (r, confirmed) => call("set_policy", { ...r, confirmed }),
    removePolicy: (r, confirmed) => call("remove_policy", { agent: r.agent, resource: r.resource, target: r.target, confirmed }),
    copyValue: (item, field) => call("copy_value", { item, field }),
    copyPlaceholder: (text) => call("copy_placeholder", { text }),
    openSession: (id) => call("open_session", { id }),
    stopSession: (id) => call("stop_session", { id }),
    recoverSession: (id, browserClosed) => call("recover_session", { id, browserClosed }),
    kinds: () => call("kinds"),
    addItem: (form) => call("add_item", { form }),
    editItem: (name, form, confirmed) => call("edit_item", { name, form, confirmed }),
    removeItem: (name) => call("remove_item", { name }),
  };
}
