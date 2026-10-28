// An agent session's name, checked when it is made.

import { type Result, err, ok } from "./result.js";

declare const brand: unique symbol;
/** A session name that passed `parseSessionName`. */
export type SessionName = string & { readonly [brand]: "SessionName" };

const SESSION_NAME = /^[a-z0-9][a-z0-9_-]{0,63}$/;

export function parseSessionName(value: string): Result<SessionName> {
  if (!SESSION_NAME.test(value)) return err("bad_session");
  return ok(value as SessionName);
}
