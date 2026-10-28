// Our cookie → CDP `Network.CookieParam`, the shape the supervisor passes to `Network.setCookies`
// in the agent's profile. Which cookies go over is the domain's (`selectSiteCookies`).

import type { SiteCookie } from "../../domain/cookie.js";

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

/**
 * Translates one cookie.
 * - Host-only cookies get `url` and no `domain`: CDP turns any `domain` into a domain cookie
 *   (`.github.com`), which would widen it to every subdomain. Domain cookies keep their domain.
 * - `SameSite=None` without `Secure` is dropped to the browser default; Chrome rejects the pair.
 */
export function toCookieParam(cookie: SiteCookie): CdpCookieParam {
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
  const topLevelSite = cookie.partitionKey?.topLevelSite;
  if (topLevelSite) param.partitionKey = { topLevelSite, hasCrossSiteAncestor: cookie.partitionKey?.hasCrossSiteAncestor ?? false };
  return param;
}
