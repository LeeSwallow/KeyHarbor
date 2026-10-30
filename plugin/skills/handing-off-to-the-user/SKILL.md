---
name: handing-off-to-the-user
description: Use when a page needs a person - a CAPTCHA, a 2FA code, accepting terms, confirming a payment, an unusual sign-in check. Hands the browser window to the user, waits until they say they are done, then continues.
---

# Handing off to the user

1. `session_wait_auth {id, origin?}` returns a `request_id`. The session is now the user's:
   your input tools are refused until they resume.
2. Tell the user exactly what to do in the browser window and how to resume:
   `keyharbor resume <session-id> <request_id>`.
3. `session_wait_for_resume {id, request_id, timeout_seconds}` (1-300). On a timeout, ask
   whether they need more time and wait again; the request stays open.
4. Check the page with `browser_snapshot`; resuming does not prove the step succeeded.

- Never try to solve a CAPTCHA or guess a code.
- If the user signed in by typing their password, KeyHarbor may keep it as a draft (when
  capture is on). Record `site_state_set ... logged_in` afterwards.

## When the user is already signed in elsewhere

If the user is signed in to the site in their own Chrome, ask them to send it instead of
signing in again: "open github.com in Chrome, click the KeyHarbor extension and send it to
the `<session name>` session". Then `session_wait_site {id, origin?, timeout_seconds}`
(1-300) waits until it arrives and returns `received` with the origin, or `waiting` if the
time ran out. The session can then only load that site (and others granted); no cookie
reaches you.
