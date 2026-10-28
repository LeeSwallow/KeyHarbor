// A browser cookie in our words, and which of a site's cookies are handed over. The shape the
// supervisor sets (CDP `Network.CookieParam`) is the adapter's (adapters/wire/cdp-cookie.ts).

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

/** Why a cookie is not handed over. */
export type Skip = "out_of_scope" | "other_partition" | "other_first_party";

/**
 * Why one cookie stays behind, or null when it goes over.
 * - A partitioned cookie goes over only when its top-level site is this site (Firefox Total
 *   Cookie Protection keeps third-party cookies per top-level site; those of other sites stay).
 * - With Firefox first-party isolation, only cookies isolated to this site go over.
 */
export function whyLeftBehind(cookie: SiteCookie, scope: SiteScope): Skip | null {
  if (!inScope(scope, cookie.domain)) return "out_of_scope";
  const topLevelSite = cookie.partitionKey?.topLevelSite;
  if (topLevelSite && !isSitePartition(scope, topLevelSite)) return "other_partition";
  if (cookie.firstPartyDomain && cookie.firstPartyDomain !== scope.site) return "other_first_party";
  return null;
}

/** The site's cookies that go over; counts what stayed behind (counts only, never names or values). */
export function selectSiteCookies(cookies: readonly SiteCookie[], scope: SiteScope): { cookies: SiteCookie[]; skipped: Record<Skip, number> } {
  const skipped: Record<Skip, number> = { out_of_scope: 0, other_partition: 0, other_first_party: 0 };
  const kept: SiteCookie[] = [];
  for (const c of cookies) {
    const why = whyLeftBehind(c, scope);
    if (why) skipped[why] += 1;
    else kept.push(c);
  }
  return { cookies: kept, skipped };
}
