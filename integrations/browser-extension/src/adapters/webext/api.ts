// The slice of the WebExtension API KeyHarbor uses, typed by hand. Chrome's `chrome.*` (MV3,
// promise-returning) and Firefox's `browser.*` both satisfy it; browser differences are in the
// cookie queries each adapter passes (adapters/chrome, adapters/firefox).

/** A cookie as `cookies.getAll` returns it. Wire shape: only the cookie adapter reads it. */
export interface CookieWire {
  name: string;
  value: string;
  domain: string;
  hostOnly: boolean;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  sameSite?: string;
  session: boolean;
  expirationDate?: number;
  storeId?: string;
  partitionKey?: { topLevelSite?: string; hasCrossSiteAncestor?: boolean } | null;
  firstPartyDomain?: string;
}

export interface TabWire {
  id?: number;
  url?: string;
}

export type MessageListener = (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => boolean | undefined;

export interface WebExtApi {
  cookies: { getAll(details: Record<string, unknown>): Promise<CookieWire[]> };
  tabs: {
    query(query: { active: true; currentWindow: true }): Promise<TabWire[]>;
    get(tabId: number): Promise<TabWire>;
  };
  scripting: {
    // Without frameIds the target is the top frame only.
    executeScript<A extends unknown[]>(injection: { target: { tabId: number }; func: (...args: A) => unknown; args?: A }): Promise<Array<{ result?: unknown }>>;
  };
  runtime: {
    sendNativeMessage(application: string, message: unknown): Promise<unknown>;
    sendMessage(message: unknown): Promise<unknown>;
    onMessage: { addListener(listener: MessageListener): void };
  };
  permissions: { request(permissions: { origins: string[] }): Promise<boolean> };
  storage: { local: { get(key: string): Promise<Record<string, unknown>>; set(items: Record<string, unknown>): Promise<void> } };
}
