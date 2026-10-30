// Use case: answer one ask of the KeyHarbor app to hand a site to a session (drag to hand off,
// spec 2026-10-31). The ask may come from anything on this computer, so the person decides in the
// browser: nothing goes over unless they press the notification. Asks for sites the extension has
// no access to yet are answered without asking (access can only be granted inside a click).

import { handOffRequest } from "../domain/handoff.js";
import { hostPatterns, scopeOf } from "../domain/scope.js";
import type { SiteRequestOutcome } from "../domain/site-request.js";
import type { Ports } from "../ports/index.js";
import { makeHandOffScope } from "./hand-off-site.js";

/** A dragged site is handed over for one hour, the popup's shortest choice. */
const DRAG_TTL_SECONDS = 3600;

export function makeAnswerSiteRequest(ports: Ports) {
  const handOffScope = makeHandOffScope(ports);
  return async (origin: string, session: string): Promise<SiteRequestOutcome> => {
    const scope = scopeOf(origin);
    if (!scope.ok) return { outcome: "bad_site" };
    const request = handOffRequest(session, DRAG_TTL_SECONDS, false);
    if (!request.ok) return { outcome: "no_session" };
    if (!(await ports.permissions.has(hostPatterns(scope.value)).catch(() => false))) return { outcome: "no_permission" };
    if (!(await ports.notifier.askHandOff(scope.value.origin, request.value.session).catch(() => false))) return { outcome: "declined" };
    const handed = await handOffScope(scope.value, request.value);
    if (handed.ok) return { outcome: "handed_off", cookies: handed.value.cookies };
    return { outcome: handed.error === "no_permission" ? "no_permission" : handed.error === "host_refused" ? "no_session" : "failed" };
  };
}
