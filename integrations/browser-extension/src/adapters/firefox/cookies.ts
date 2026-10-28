// Firefox: one query covers every partition (Total Cookie Protection, `partitionKey: {}`) and
// every first-party domain (first-party isolation, `firstPartyDomain: null`). Without
// `firstPartyDomain` the call throws when isolation is on.

import type { CookieQueries } from "../webext/cookies.js";

export const firefoxCookieQueries: CookieQueries = (scope) => [{ domain: scope.site, partitionKey: {}, firstPartyDomain: null }];
