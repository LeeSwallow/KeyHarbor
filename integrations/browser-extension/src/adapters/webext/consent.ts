// Permissions and the browser notification that asks the person before a dragged site goes over
// (spec 2026-10-31). Chrome shows a [넘기기] button; Firefox has no notification buttons, so a
// click on the notification itself is the yes. Dismissing it, or two minutes passing, is a no.

import type { Notifier, Permissions } from "../../ports/index.js";
import type { WebExtApi } from "./api.js";

/** How long a notification waits for the person. */
const ASK_MS = 120_000;

export const webextPermissions = (api: WebExtApi): Permissions => ({
  has: (origins) => api.permissions.contains({ origins }),
});

export function webextNotifier(api: WebExtApi, opts: { buttons: boolean; korean: boolean }): Notifier {
  const waiting = new Map<string, (yes: boolean) => void>();
  const settle = (id: string, yes: boolean) => {
    waiting.get(id)?.(yes);
    waiting.delete(id);
    void api.notifications.clear(id).catch(() => undefined);
  };
  if (opts.buttons) api.notifications.onButtonClicked?.addListener((id, button) => settle(id, button === 0));
  else api.notifications.onClicked.addListener((id) => settle(id, true));
  api.notifications.onClosed.addListener((id) => settle(id, false));
  let next = 0;
  return {
    askHandOff(origin, session) {
      const id = `keyharbor-hand-off-${++next}`;
      const title = opts.korean ? "KeyHarbor: 사이트 넘기기" : "KeyHarbor: hand off a site";
      const message = opts.korean
        ? `${origin}의 로그인을 ${session} 세션에 넘길까요?${opts.buttons ? "" : " 이 알림을 누르면 넘깁니다."}`
        : `Hand your ${origin} login to the ${session} session?${opts.buttons ? "" : " Click this notification to hand it off."}`;
      const answer = new Promise<boolean>((resolve) => {
        waiting.set(id, resolve);
        setTimeout(() => settle(id, false), ASK_MS);
      });
      void api.notifications
        .create(id, {
          type: "basic",
          iconUrl: api.runtime.getURL("icon-128.png"),
          title,
          message,
          requireInteraction: true,
          ...(opts.buttons ? { buttons: [{ title: opts.korean ? "넘기기" : "Hand off" }] } : {}),
        })
        .catch(() => settle(id, false));
      return answer;
    },
  };
}
