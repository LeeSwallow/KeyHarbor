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

/** A native port (`runtime.connectNative`). Wire shape: only the bridge adapter uses it. */
export interface NativePortWire {
  postMessage(message: unknown): void;
  onMessage: { addListener(listener: (message: unknown) => void): void };
  onDisconnect: { addListener(listener: () => void): void };
}

export interface NotificationOptionsWire {
  type: "basic";
  iconUrl: string;
  title: string;
  message: string;
  requireInteraction?: boolean;
  buttons?: Array<{ title: string }>;
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
    connectNative(application: string): NativePortWire;
    getURL(path: string): string;
    sendMessage(message: unknown): Promise<unknown>;
    onMessage: { addListener(listener: MessageListener): void };
  };
  permissions: {
    request(permissions: { origins: string[] }): Promise<boolean>;
    contains(permissions: { origins: string[] }): Promise<boolean>;
  };
  notifications: {
    create(id: string, options: NotificationOptionsWire): Promise<string>;
    clear(id: string): Promise<boolean>;
    onClicked: { addListener(listener: (id: string) => void): void };
    onButtonClicked?: { addListener(listener: (id: string, button: number) => void): void };
    onClosed: { addListener(listener: (id: string) => void): void };
  };
  storage: { local: { get(key: string): Promise<Record<string, unknown>>; set(items: Record<string, unknown>): Promise<void> } };
}
