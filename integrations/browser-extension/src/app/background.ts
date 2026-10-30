// The background's one job: answer checked popup commands by running the use case. Answers carry
// counts and names; the values of a fill go from the host into the page and stop there.

import type { LoginChoice, SaveOutcome } from "../domain/login.js";
import type { Result } from "../domain/result.js";
import type { Ports } from "../ports/index.js";
import { makeFillLogin, makeListLogins, makeSaveLogin } from "./autofill.js";
import type { Command } from "./commands.js";
import { type HandOffOutcome, makeHandOffSite } from "./hand-off-site.js";

export type CommandAnswer = HandOffOutcome | LoginChoice[] | { item: string } | SaveOutcome;
export type CommandHandler = (command: Command) => Promise<Result<CommandAnswer>>;

export function makeCommandHandler(ports: Ports): CommandHandler {
  const handOffSite = makeHandOffSite(ports);
  const listLogins = makeListLogins(ports);
  const fillLogin = makeFillLogin(ports);
  const saveLogin = makeSaveLogin(ports);
  return (command) => {
    switch (command.type) {
      case "hand_off":
        return handOffSite(command.tabId, command.request);
      case "list_logins":
        return listLogins(command.tabId);
      case "fill_login":
        return fillLogin(command.tabId, command.item);
      case "save_login":
        return saveLogin(command.tabId);
    }
  };
}
