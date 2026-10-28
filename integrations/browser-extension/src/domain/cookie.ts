// A browser cookie in our words, and its translation to CDP `Network.CookieParam`, the shape the
// supervisor passes to `Network.setCookies` in the agent's profile.

import { type SiteScope, inScope, isSitePartition } from "./scope.js";

/** A cookie as the WebExtension cookies API describes it (both browsers), as plain data. */
export interface SiteCookie {
  name: string;
  value: string;
  /** ".github.com" for a domain cookie, "github.com" for a host-only one. */
  domain: string;
  hostOnly: boolean;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  sameSite: "no_restriction" | "lax" | "strict" | "unspecified";
  session: boolean;
  /** Seconds since the epoch; absent for session cookies. */
  expirationDate?: number;
  /** CHIPS / Firefox Total Cookie Protection partition. Absent for unpartitioned cookies. */
  partitionKey?: { topLevelSite?: string; hasCrossSiteAncestor?: boolean };
  /** Firefox first-party isolation. Empty or absent when it is off. */
  firstPartyDomain?: string;
}

/** CDP `Network.CookieParam` (the fields KeyHarbor sets). */
export interface CdpCookieParam {
  name: string;
  value: string;
  url: string;
  domain?: string;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  sameSite?: "Strict" | "Lax" | "None";
  expires?: number;
  partitionKey?: { topLevelSite: string; hasCrossSiteAncestor: boolean };
}

const SAME_SITE: Record<SiteCookie["sameSite"], CdpCookieParam["sameSite"] | undefined> = {
  no_restriction: "None",
  lax: "Lax",
  strict: "Strict",
  unspecified: undefined,
};

/** Why a cookie is not handed over. */
export type Skip = "out_of_scope" | "other_partition" | "other_first_party";

/**
 * Translates one cookie, or says why it stays behind.
 * - Host-only cookies get `url` and no `domain`: CDP turns any `domain` into a domain cookie
 *   (`.github.com`), which would widen it to every subdomain. Domain cookies keep their domain.
 * - `SameSite=None` without `Secure` is dropped to the browser default; Chrome rejects the pair.
 * - A partitioned cookie goes over only when its top-level site is this site (Firefox Total
 *   Cookie Protection keeps third-party cookies per top-level site; those of other sites stay).
 * - With Firefox first-party isolation, only cookies isolated to this site go over.
 */
export function toCookieParam(cookie: SiteCookie, scope: SiteScope): CdpCookieParam | Skip {
  if (!inScope(scope, cookie.domain)) return "out_of_scope";
  const topLevelSite = cookie.partitionKey?.topLevelSite;
  if (topLevelSite && !isSitePartition(scope, topLevelSite)) return "other_partition";
  if (cookie.firstPartyDomain && cookie.firstPartyDomain !== scope.site) return "other_first_party";

  const host = cookie.domain.replace(/^\./, "");
  const param: CdpCookieParam = {
    name: cookie.name,
    value: cookie.value,
    url: `${cookie.secure ? "https" : "http"}://${host}${cookie.path || "/"}`,
    path: cookie.path || "/",
    secure: cookie.secure,
    httpOnly: cookie.httpOnly,
  };
  if (!cookie.hostOnly) param.domain = cookie.domain.startsWith(".") ? cookie.domain : `.${cookie.domain}`;
  const sameSite = SAME_SITE[cookie.sameSite];
  if (sameSite && !(sameSite === "None" && !cookie.secure)) param.sameSite = sameSite;
  if (!cookie.session && cookie.expirationDate !== undefined) param.expires = Math.floor(cookie.expirationDate);
  if (topLevelSite) param.partitionKey = { topLevelSite, hasCrossSiteAncestor: cookie.partitionKey?.hasCrossSiteAncestor ?? false };
  return param;
}

/** Translates a site's cookies; counts what stayed behind (counts only, never names or values). */
export function toCookieParams(cookies: readonly SiteCookie[], scope: SiteScope): { params: CdpCookieParam[]; skipped: Record<Skip, number> } {
  const skipped: Record<Skip, number> = { out_of_scope: 0, other_partition: 0, other_first_party: 0 };
  const params: CdpCookieParam[] = [];
  for (const c of cookies) {
    const r = toCookieParam(c, scope);
    if (typeof r === "string") skipped[r] += 1;
    else params.push(r);
  }
  return { params, skipped };
}
