// An agent session's name. The rule is Rust's `SessionName::parse` (crates/domain), the
// judge once the native host checks it; both sides test against contracts/session-names.json.

import { type Result, err, ok } from "./result.js";

declare const brand: unique symbol;
/** A trimmed session name of 1–120 UTF-8 bytes with no control character. */
export type SessionName = string & { readonly [brand]: "SessionName" };

const MAX_BYTES = 120;
// Rust's `str::trim` strips Unicode White_Space (U+0085 included, U+FEFF not); JS `trim` differs.
const EDGE_WHITE_SPACE = /^\p{White_Space}+|\p{White_Space}+$/gu;
// Rust's `char::is_control` is general category Cc.
const CONTROL = /\p{Cc}/u;
// A lone surrogate cannot be in a Rust `str`, so it cannot be in a name either.
const LONE_SURROGATE = /\p{Cs}/u;
const encoder = new TextEncoder();

export function parseSessionName(value: string): Result<SessionName> {
  const name = value.replace(EDGE_WHITE_SPACE, "");
  if (name === "" || encoder.encode(name).length > MAX_BYTES || CONTROL.test(name) || LONE_SURROGATE.test(name)) return err("bad_session");
  return ok(name as SessionName);
}
