// What the popup can ask the background, in our words. The message shape and its validation are
// the adapter's (adapters/wire/command.ts).

import type { HandOffRequest } from "../domain/handoff.js";

/** A checked command. A new command is a new member here. */
export type Command =
  | { type: "hand_off"; tabId: number; request: HandOffRequest }
  | { type: "list_logins"; tabId: number }
  | { type: "fill_login"; tabId: number; item: string }
  | { type: "save_login"; tabId: number };

/** What the popup asks for, before any check. */
export interface HandOffAsk {
  tabId: number;
  session: string;
  ttlSeconds: number;
  includeLocalStorage: boolean;
}
