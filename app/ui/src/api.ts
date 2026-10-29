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
  };
}
