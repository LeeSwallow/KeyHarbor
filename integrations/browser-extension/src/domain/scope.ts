// Which cookies belong to "this site": the registrable domain of the tab (eTLD+1, from the
// Public Suffix List) and every subdomain of it. github.com covers gist.github.com; a page on
// foo.github.io covers only foo.github.io, because github.io is a public suffix.

import { getDomain } from "tldts";
import { type Result, err, ok } from "./result.js";

export interface SiteScope {
  /** "https://github.com" — what the host opens. */
  origin: string;
  /** "github.com" — the tab's host. */
  host: string;
  /** "github.com" — the registrable domain, or the host itself for IPs and localhost. */
  site: string;
}

const ALLOWED_SCHEMES = new Set(["http:", "https:"]);

/** The scope of a tab URL. Only http(s) pages have one; file:, about:, browser and extension pages do not. */
export function scopeOf(url: string): Result<SiteScope> {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return err("unsupported_page");
  }
  if (!ALLOWED_SCHEMES.has(parsed.protocol) || parsed.hostname === "") return err("unsupported_page");
  const host = parsed.hostname.toLowerCase();
  const site = getDomain(host, { allowPrivateDomains: true }) ?? host;
  return ok({ origin: parsed.origin, host, site });
}

/** Whether a cookie domain (".github.com", "gist.github.com") is the site or under it. */
export function inScope(scope: SiteScope, cookieDomain: string): boolean {
  const d = cookieDomain.replace(/^\./, "").toLowerCase();
  return d === scope.site || d.endsWith(`.${scope.site}`);
}

/** Whether a partition's top-level site ("https://github.com") is this site. */
export function isSitePartition(scope: SiteScope, topLevelSite: string): boolean {
  try {
    return inScope(scope, new URL(topLevelSite).hostname);
  } catch {
    return false;
  }
}

/** The host-permission patterns to ask for: the site and all its subdomains, both schemes. */
export function hostPatterns(scope: SiteScope): string[] {
  const isAddressOrSingleLabel = /^[\d.]+$/.test(scope.site) || scope.site.includes(":") || !scope.site.includes(".");
  const exact = `*://${scope.site.includes(":") ? `[${scope.site}]` : scope.site}/*`;
  return isAddressOrSingleLabel ? [exact] : [exact, `*://*.${scope.site}/*`];
}
