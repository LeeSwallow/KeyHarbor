// Wires the command handler to `runtime.onMessage`. Answers through sendResponse so Chrome's
// service worker and Firefox's background script behave the same.

import type { CommandHandler } from "../../app/background.js";
import type { HandOffOutcome } from "../../app/hand-off-site.js";
import type { Result } from "../../domain/result.js";
import { parseCommand } from "../wire/command.js";
import type { WebExtApi } from "./api.js";

/** Reads a raw popup message and runs it; a message that is not a command runs nothing. */
export function answerCommand(handle: CommandHandler): (raw: unknown) => Promise<Result<HandOffOutcome>> {
  return async (raw) => {
    const command = parseCommand(raw);
    return command.ok ? handle(command.value) : command;
  };
}

export function listenForCommands(api: WebExtApi, handle: CommandHandler): void {
  const answer = answerCommand(handle);
  api.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    answer(message).then(sendResponse, () => sendResponse({ ok: false, error: "bad_command" }));
    return true; // the response is sent asynchronously
  });
}
