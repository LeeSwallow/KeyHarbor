// The background's one job: answer popup commands by running the use case.

import type { Result } from "../domain/result.js";
import type { Ports } from "../ports/index.js";
import { parseCommand } from "./commands.js";
import { type HandOffOutcome, makeHandOffSite } from "./hand-off-site.js";

export type CommandHandler = (raw: unknown) => Promise<Result<HandOffOutcome>>;

export function makeCommandHandler(ports: Ports): CommandHandler {
  const handOffSite = makeHandOffSite(ports);
  return async (raw) => {
    const command = parseCommand(raw);
    if (!command.ok) return command;
    return handOffSite(command.value.tabId, command.value.request);
  };
}
