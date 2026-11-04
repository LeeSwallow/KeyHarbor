// The background, shared by both browsers: wires the command handler to `runtime.onMessage` and
// opens the bridge to the KeyHarbor app. Answers through sendResponse so Chrome's service worker
// and Firefox's background script behave the same.

import { type CommandAnswer, type CommandHandler, makeCommandHandler } from "../../app/background.js";
import { makeAnswerSiteRequest } from "../../app/site-request.js";
import { type Result, err } from "../../domain/result.js";
import { parseCommand } from "../wire/command.js";
import type { WebExtApi } from "./api.js";
import { startBridge } from "./bridge.js";
import { type BrowserTraits, webextPorts } from "./ports.js";

/** Reads a raw popup message and runs it; a message that is not a command runs nothing. */
export function answerCommand(handle: CommandHandler): (raw: unknown) => Promise<Result<CommandAnswer>> {
  return async (raw) => {
    const command = parseCommand(raw);
    return command.ok ? handle(command.value) : command;
  };
}

export function listenForCommands(api: WebExtApi, handle: CommandHandler): void {
  const answer = answerCommand(handle);
  api.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    answer(message).then(sendResponse, () => sendResponse(err("bad_command")));
    return true; // the response is sent asynchronously
  });
}

/** Everything a background entry point does once it has its browser's namespace. */
export function startBackground(api: WebExtApi, traits: BrowserTraits): void {
  const ports = webextPorts(api, traits);
  listenForCommands(api, makeCommandHandler(ports));
  // The KeyHarbor app's asks to hand a dragged site over (spec 2026-10-31).
  startBridge(api, makeAnswerSiteRequest(ports));
}
