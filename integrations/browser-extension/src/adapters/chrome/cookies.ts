// Chrome: unpartitioned cookies by domain, plus CHIPS cookies partitioned under this site.

import type { CookieQueries } from "../webext/cookies.js";

export const chromeCookieQueries: CookieQueries = (scope) => [
  { domain: scope.site },
  { domain: scope.site, partitionKey: { topLevelSite: `https://${scope.site}` } },
];
