// Messages from the popup to the background, and their one validation. The popup holds no
// cookie and no password: it names a tab (and a session or a login); the background does the rest.

import type { Command, HandOffAsk } from "../../app/commands.js";
import { handOffRequest } from "../../domain/handoff.js";
import { type Result, err, ok } from "../../domain/result.js";

/** The raw shapes the popup posts with runtime.sendMessage. */
export interface HandOffCommandWire {
  type: "hand_off";
  tabId: number;
  session: string;
  ttlSeconds: number;
  includeLocalStorage: boolean;
}
export type LoginCommandWire =
  | { type: "list_logins"; tabId: number }
  | { type: "fill_login"; tabId: number; item: string }
  | { type: "save_login"; tabId: number };

export const toHandOffCommandWire = (ask: HandOffAsk): HandOffCommandWire => ({ type: "hand_off", ...ask });

/** Item names as the vault allows them: 1–64 letters, digits, '-', '_', '/' (no draft). */
const ITEM = /^[\p{L}\p{N}_\-/]{1,64}$/u;

export function parseCommand(raw: unknown): Result<Command> {
  if (typeof raw !== "object" || raw === null) return err("bad_command");
  const m = raw as Record<string, unknown>;
  if (!Number.isInteger(m.tabId)) return err("bad_command");
  const tabId = m.tabId as number;
  switch (m.type) {
    case "hand_off": {
      const request = handOffRequest(String(m.session ?? ""), Number(m.ttlSeconds), m.includeLocalStorage === true);
      return request.ok ? ok({ type: "hand_off", tabId, request: request.value }) : request;
    }
    case "list_logins":
      return ok({ type: "list_logins", tabId });
    case "fill_login":
      return typeof m.item === "string" && ITEM.test(m.item) ? ok({ type: "fill_login", tabId, item: m.item }) : err("bad_command");
    case "save_login":
      return ok({ type: "save_login", tabId });
    default:
      return err("bad_command");
  }
}
