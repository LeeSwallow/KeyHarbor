// Wires the command handler to `runtime.onMessage`. Answers through sendResponse so Chrome's
// service worker and Firefox's background script behave the same.

import type { CommandHandler } from "../../app/background.js";
import type { WebExtApi } from "./api.js";

export function listenForCommands(api: WebExtApi, handle: CommandHandler): void {
  api.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    handle(message).then(sendResponse, () => sendResponse({ ok: false, error: "bad_command" }));
    return true; // the response is sent asynchronously
  });
}
