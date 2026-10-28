// CookieSource over `cookies.getAll`. Each browser says which queries cover a site.

import type { SiteCookie } from "../../domain/cookie.js";
import type { SiteScope } from "../../domain/scope.js";
import type { CookieSource } from "../../ports/index.js";
import type { CookieWire, WebExtApi } from "./api.js";

/** The `getAll` queries that together return every cookie of a site. */
export type CookieQueries = (scope: SiteScope) => Array<Record<string, unknown>>;

const SAME_SITE = new Set(["no_restriction", "lax", "strict"]);

/** Translates the browser's cookie into ours. */
export function toSiteCookie(c: CookieWire): SiteCookie {
  const cookie: SiteCookie = {
    name: c.name,
    value: c.value,
    domain: c.domain,
    hostOnly: c.hostOnly,
    path: c.path,
    secure: c.secure,
    httpOnly: c.httpOnly,
    sameSite: SAME_SITE.has(c.sameSite ?? "") ? (c.sameSite as SiteCookie["sameSite"]) : "unspecified",
    session: c.session,
  };
  if (c.expirationDate !== undefined) cookie.expirationDate = c.expirationDate;
  if (c.partitionKey?.topLevelSite) {
    cookie.partitionKey = { topLevelSite: c.partitionKey.topLevelSite, hasCrossSiteAncestor: c.partitionKey.hasCrossSiteAncestor ?? false };
  }
  if (c.firstPartyDomain) cookie.firstPartyDomain = c.firstPartyDomain;
  return cookie;
}

const identity = (c: CookieWire) => [c.name, c.domain, c.path, c.partitionKey?.topLevelSite ?? "", c.firstPartyDomain ?? ""].join("\u0000");

export function webextCookieSource(api: WebExtApi, queries: CookieQueries): CookieSource {
  return {
    async readSiteCookies(scope) {
      const batches = await Promise.all(queries(scope).map((q) => api.cookies.getAll(q)));
      const seen = new Map<string, CookieWire>();
      for (const c of batches.flat()) seen.set(identity(c), c);
      return [...seen.values()].map(toSiteCookie);
    },
  };
}
