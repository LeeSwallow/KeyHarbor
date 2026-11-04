// Messages between the popup and the background, and their one validation, both ways. The popup
// holds no cookie and no password: it names a tab (and a session or a login); the background
// does the rest and answers with counts and names.

import type { Command, HandOffAsk } from "../../app/commands.js";
import type { HandOffOutcome } from "../../app/hand-off-site.js";
import { handOffRequest } from "../../domain/handoff.js";
import { isFillableItemName } from "../../domain/item-name.js";
import type { LoginChoice, SaveOutcome } from "../../domain/login.js";
import { type Result, err, isErrorCode, ok } from "../../domain/result.js";

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
export type CommandWire = HandOffCommandWire | LoginCommandWire;
export type CommandType = CommandWire["type"];

type WireOf<K extends CommandType> = Extract<CommandWire, { type: K }>;

export const toHandOffCommandWire = (ask: HandOffAsk): HandOffCommandWire => ({ type: "hand_off", ...ask });
export const toListLoginsWire = (tabId: number): WireOf<"list_logins"> => ({ type: "list_logins", tabId });
export const toFillLoginWire = (tabId: number, item: string): WireOf<"fill_login"> => ({ type: "fill_login", tabId, item });
export const toSaveLoginWire = (tabId: number): WireOf<"save_login"> => ({ type: "save_login", tabId });

export function parseCommand(raw: unknown): Result<Command> {
  if (typeof raw !== "object" || raw === null) return err("bad_command");
  const m = raw as Record<string, unknown>;
  if (typeof m.tabId !== "number" || !Number.isInteger(m.tabId)) return err("bad_command");
  const tabId = m.tabId;
  switch (m.type) {
    case "hand_off": {
      if (typeof m.session !== "string" || typeof m.ttlSeconds !== "number" || typeof m.includeLocalStorage !== "boolean") return err("bad_command");
      const request = handOffRequest(m.session, m.ttlSeconds, m.includeLocalStorage);
      return request.ok ? ok({ type: "hand_off", tabId, request: request.value }) : request;
    }
    case "list_logins":
      return ok({ type: "list_logins", tabId });
    case "fill_login":
      return typeof m.item === "string" && isFillableItemName(m.item) ? ok({ type: "fill_login", tabId, item: m.item }) : err("bad_command");
    case "save_login":
      return ok({ type: "save_login", tabId });
    default:
      return err("bad_command");
  }
}

/** What each command answers with when it succeeds. */
export interface AnswerOf {
  hand_off: HandOffOutcome;
  list_logins: LoginChoice[];
  fill_login: { item: string };
  save_login: SaveOutcome;
}
type Fields = Record<string, unknown>;
const isFields = (v: unknown): v is Fields => typeof v === "object" && v !== null && !Array.isArray(v);
const isCount = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0;
const isNamed = (v: unknown): v is { item: string } => isFields(v) && typeof v.item === "string";

/** One check per command's answer: a new command is a new row (and a type error until it has one). */
const ANSWER_CHECKS: { [K in CommandType]: (value: unknown) => value is AnswerOf[K] } = {
  hand_off: (v): v is HandOffOutcome =>
    isFields(v) && typeof v.session === "string" && typeof v.origin === "string" && isCount(v.cookies) && typeof v.localStorage === "boolean",
  list_logins: (v): v is LoginChoice[] => Array.isArray(v) && v.every(isNamed),
  fill_login: (v): v is { item: string } => isNamed(v),
  save_login: (v): v is SaveOutcome => isFields(v) && typeof v.item === "string" && (v.kind === "saved" || v.kind === "known"),
};

/** Reads the background's answer to a `type` command. Anything that is not a `Result` of the
 *  right shape, or an error with an unknown code, is `bad_reply`. */
export function parseCommandAnswer<K extends CommandType>(type: K, raw: unknown): Result<AnswerOf[K]> {
  if (!isFields(raw)) return err("bad_reply");
  if (raw.ok === false) return isErrorCode(raw.error) ? err(raw.error) : err("bad_reply");
  if (raw.ok === true && ANSWER_CHECKS[type](raw.value)) return ok(raw.value);
  return err("bad_reply");
}
