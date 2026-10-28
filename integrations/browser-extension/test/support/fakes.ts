// Test doubles. They follow the port contracts and call the domain; they copy no rules. The
// host is the real wire adapter over a fake transport, so tests see the message that would go out.

import type { SiteCookie } from "../../src/domain/cookie.js";
import { nativeHostOver } from "../../src/adapters/wire/native-host.js";
import type { HostRequestWire } from "../../src/adapters/wire/protocol.js";
import type { Ports, Tab } from "../../src/ports/index.js";

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
  sent: HostRequestWire[];
}

export function fakePorts(opts: { tab?: Tab | null; cookies?: SiteCookie[] | "throws"; storage?: Record<string, string> | "throws"; reply?: unknown | "throws" } = {}): FakePorts {
  const sent: HostRequestWire[] = [];
  const tab = opts.tab === undefined ? { id: 7, url: "https://github.com/settings" } : opts.tab;
  return {
    sent,
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
      return opts.reply ?? { v: 1, type: "received", session: request.session, cookies: request.cookies.length };
    }),
  };
}
