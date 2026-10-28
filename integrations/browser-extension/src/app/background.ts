// The background's one job: answer checked popup commands by running the use case.

import type { Result } from "../domain/result.js";
import type { Ports } from "../ports/index.js";
import type { Command } from "./commands.js";
import { type HandOffOutcome, makeHandOffSite } from "./hand-off-site.js";

export type CommandHandler = (command: Command) => Promise<Result<HandOffOutcome>>;

export function makeCommandHandler(ports: Ports): CommandHandler {
  const handOffSite = makeHandOffSite(ports);
  return (command) => handOffSite(command.tabId, command.request);
}
