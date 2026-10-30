// Logins in the person's own browser (phase 7, ADR-0017): which saved logins may fill this site,
// the values the host returns after Touch ID, and what the page held when the person pressed save.
// Values live only inside `LoginValues` and only for the moment they are put into the page.

/** A saved login offered for this site: its name only. */
export interface LoginChoice {
  item: string;
}

/** A username and password on their way into, or out of, one page. */
export interface LoginValues {
  username?: string;
  password: string;
}

/** What the host did with a login the person saved. */
export type SaveOutcome = { kind: "saved"; item: string } | { kind: "known"; item: string };
