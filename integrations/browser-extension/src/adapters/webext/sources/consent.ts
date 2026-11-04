// Permissions and the browser notification that asks the person before a dragged site goes over
// (spec 2026-10-31). Chrome shows a [넘기기] button; Firefox has no notification buttons, so a
// click on the notification itself is the yes. Dismissing it, or two minutes passing, is a no.

import type { Notifier, Permissions } from "../../../ports/index.js";
import type { Lang } from "../../../ui/popup/i18n.js";
import type { WebExtApi } from "../api.js";
import { CONSENT_TEXT, type ConsentGesture, consentMessage } from "./consent-text.js";

/**
 * How long a notification waits for the person. Fixed below the app's BRIDGE_WAIT (130 s,
 * crates/supervisor/src/bridge.rs) so the no reaches the app before it stops waiting.
 */
const ASK_MS = 120_000;

/** Notification ids start with this, then a counter. */
const NOTIFICATION_PREFIX = "keyharbor-hand-off-";

/** One of the icons manifest/base.json lists; the build copies it to the extension root. */
export const NOTIFICATION_ICON = "icon-128.png";

export const webextPermissions = (api: WebExtApi): Permissions => ({
  has: (origins) => api.permissions.contains({ origins }),
});

export function webextNotifier(api: WebExtApi, opts: { gesture: ConsentGesture; lang: Lang }, askMs: number = ASK_MS): Notifier {
  const waiting = new Map<string, { resolve: (yes: boolean) => void; timer: ReturnType<typeof setTimeout> }>();
  const settle = (id: string, yes: boolean) => {
    const ask = waiting.get(id);
    if (!ask) return;
    waiting.delete(id);
    clearTimeout(ask.timer);
    ask.resolve(yes);
    void api.notifications.clear(id).catch(() => undefined);
  };
  if (opts.gesture === "button") api.notifications.onButtonClicked?.addListener((id, button) => settle(id, button === 0));
  else api.notifications.onClicked.addListener((id) => settle(id, true));
  api.notifications.onClosed.addListener((id) => settle(id, false));
  const text = CONSENT_TEXT[opts.lang];
  let next = 0;
  return {
    askHandOff(origin, session) {
      const id = `${NOTIFICATION_PREFIX}${++next}`;
      const answer = new Promise<boolean>((resolve) => {
        waiting.set(id, { resolve, timer: setTimeout(() => settle(id, false), askMs) });
      });
      void api.notifications
        .create(id, {
          type: "basic",
          iconUrl: api.runtime.getURL(NOTIFICATION_ICON),
          title: text.title,
          message: consentMessage(opts.lang, opts.gesture, origin, session),
          requireInteraction: true,
          ...(opts.gesture === "button" ? { buttons: [{ title: text.button }] } : {}),
        })
        .catch(() => settle(id, false));
      return answer;
    },
  };
}
