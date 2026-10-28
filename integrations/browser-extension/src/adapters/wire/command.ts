// Messages from the popup to the background, and their one validation. The popup holds no
// cookies: it names a tab and a session; the background reads and sends.

import type { Command, HandOffAsk } from "../../app/commands.js";
import { handOffRequest } from "../../domain/handoff.js";
import { type Result, err, ok } from "../../domain/result.js";

/** The raw shape the popup posts with runtime.sendMessage. */
export interface HandOffCommandWire {
  type: "hand_off";
  tabId: number;
  session: string;
  ttlSeconds: number;
  includeLocalStorage: boolean;
}

export const toHandOffCommandWire = (ask: HandOffAsk): HandOffCommandWire => ({ type: "hand_off", ...ask });

export function parseCommand(raw: unknown): Result<Command> {
  if (typeof raw !== "object" || raw === null) return err("bad_command");
  const m = raw as Partial<HandOffCommandWire>;
  if (m.type !== "hand_off" || !Number.isInteger(m.tabId)) return err("bad_command");
  const request = handOffRequest(String(m.session ?? ""), Number(m.ttlSeconds), m.includeLocalStorage === true);
  if (!request.ok) return request;
  return ok({ type: "hand_off", tabId: m.tabId as number, request: request.value });
}
