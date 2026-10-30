import type { HostReply, SiteHandoff } from "../domain/handoff.js";
import type { LoginChoice, LoginValues, SaveOutcome } from "../domain/login.js";
import type { Result } from "../domain/result.js";

/** The KeyHarbor native host. Every method rejects when the host is unavailable; none logs. */
export interface NativeHost {
  /** Delivers one hand-off and resolves with the host's answer, or `bad_reply`. */
  receiveSite(handoff: SiteHandoff): Promise<Result<HostReply>>;
  /** The saved logins that may fill `origin`: names only. */
  listLogins(origin: string): Promise<Result<LoginChoice[]>>;
  /** The values of `item` for `origin`, after the person confirms with Touch ID. */
  fillLogin(origin: string, item: string): Promise<Result<LoginValues>>;
  /** Stores what the page held as a draft (or says the vault already knows it). */
  saveLogin(origin: string, login: LoginValues): Promise<Result<SaveOutcome>>;
}
