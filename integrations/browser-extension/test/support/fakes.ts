// Test doubles. They follow the port contracts and call the domain; they copy no rules. The
// host is the real wire adapter over a fake transport, so tests see the message that would go out.

import type { SiteCookie } from "../../src/domain/cookie.js";
import { nativeHostOver } from "../../src/adapters/wire/native-host.js";
import type { HostRequestWire } from "../../src/adapters/wire/protocol.js";
import type { LoginValues } from "../../src/domain/login.js";
import type { Ports, Tab } from "../../src/ports/index.js";

/** A password no test output may ever contain. */
export const PASSWORD = "pw-9f2c-never-logged";

/** A cookie value no test output may ever contain. */
export const SECRET = "s3cr3t-cookie-value-9f2c";

export const cookie = (over: Partial<SiteCookie> = {}): SiteCookie => ({
  name: "user_session",
  value: SECRET,
  domain: ".github.com",
  hostOnly: false,
  path: "/",
  secure: true,
  httpOnly: true,
  sameSite: "lax",
  session: false,
  expirationDate: 1893456000.5,
  ...over,
});

export interface FakePorts extends Ports {
  /** The notifications shown: [origin, session]. */
  asked: Array<[string, string]>;
  // Loosely typed so a test reads any request's fields.
  sent: Array<HostRequestWire & Record<string, any>>;
  /** What the page received from `fill`. */
  filled: LoginValues[];
}

export function fakePorts(
  opts: {
    tab?: Tab | null;
    cookies?: SiteCookie[] | "throws";
    storage?: Record<string, string> | "throws";
    reply?: unknown | "throws";
    page?: { fill?: "filled" | "no_form" | "moved"; read?: LoginValues | null };
    /** Host access granted before (default yes). */
    granted?: boolean;
    /** What the person does with the browser notification (default: presses it). */
    pressed?: boolean;
  } = {},
): FakePorts {
  const sent: FakePorts["sent"] = [];
  const filled: LoginValues[] = [];
  const asked: Array<[string, string]> = [];
  const tab = opts.tab === undefined ? { id: 7, url: "https://github.com/settings" } : opts.tab;
  return {
    sent,
    filled,
    asked,
    permissions: { has: async () => opts.granted ?? true },
    notifier: {
      askHandOff: async (origin, session) => {
        asked.push([origin, session]);
        return opts.pressed ?? true;
      },
    },
    page: {
      fill: async (_tab, _origin, login) => {
        const answer = opts.page?.fill ?? "filled";
        if (answer === "filled") filled.push(login);
        return answer;
      },
      read: async () => (opts.page && "read" in opts.page ? (opts.page.read ?? null) : { username: "amy", password: PASSWORD }),
    },
    tabs: { active: async () => tab, find: async (id) => (tab && tab.id === id ? tab : null) },
    cookies: {
      readSiteCookies: async () => {
        if (opts.cookies === "throws") throw new Error(`boom ${SECRET}`);
        return opts.cookies ?? [cookie()];
      },
    },
    storage: {
      readLocalStorage: async () => {
        if (opts.storage === "throws") throw new Error(`boom ${SECRET}`);
        return opts.storage ?? { token: SECRET };
      },
    },
    host: nativeHostOver(async (request) => {
      sent.push(request);
      if (opts.reply === "throws") throw new Error(`host gone ${SECRET}`);
      if (opts.reply !== undefined) return opts.reply;
      switch (request.type) {
        case "receive_site":
          return { v: 1, type: "received", session: request.session, cookies: request.cookies.length };
        case "autofill_list":
          return { v: 1, type: "logins", items: ["github"] };
        case "autofill_fill":
          return { v: 1, type: "filled", username: "amy", password: PASSWORD };
        case "save_login":
          return { v: 1, type: "saved", item: "github~draft-1" };
      }
    }),
  };
}
