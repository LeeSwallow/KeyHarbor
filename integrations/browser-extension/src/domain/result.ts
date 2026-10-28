// Results and error codes. An error is a code, never a value: nothing here can carry a cookie.

export type ErrorCode =
  | "unsupported_page" // not http(s): file:, about:, browser or extension pages
  | "bad_session" // session name not 1-120 bytes, or with a control character (SessionName)
  | "bad_ttl" // ttl outside TTL_RANGE
  | "no_permission" // the person declined host access for the site
  | "no_cookies" // nothing to hand over
  | "host_unavailable" // native host not installed or crashed
  | "host_refused" // the host answered with an error
  | "bad_reply" // the host's answer is not protocol v1
  | "storage_unavailable" // localStorage could not be read
  | "bad_command"; // a popup message the background does not understand

export type Result<T> = { ok: true; value: T } | { ok: false; error: ErrorCode };

export const ok = <T>(value: T): Result<T> => ({ ok: true, value });
export const err = <T = never>(error: ErrorCode): Result<T> => ({ ok: false, error });
