import type { SiteCookie } from "../domain/cookie.js";
import type { SiteScope } from "../domain/scope.js";

/** Reads the everyday browser's cookies. */
export interface CookieSource {
  /**
   * Every cookie of the site and its subdomains that the browser lets the extension read,
   * across partitions. Returns [] when host access was not granted. Never logs.
   */
  readSiteCookies(scope: SiteScope): Promise<SiteCookie[]>;
}
