// An ask from the KeyHarbor app (through the bridge) to hand a site to a session: the person
// dragged a tab onto a session card. The extension asks the person again in the browser before
// it goes ahead (spec 2026-10-31), because anything on the same computer can write the ask.

/** How an ask ended, as the app reads it. */
export type SiteRequestOutcome =
  | { outcome: "handed_off"; cookies: number }
  | { outcome: "declined" | "no_permission" | "no_session" | "bad_site" | "failed" };
