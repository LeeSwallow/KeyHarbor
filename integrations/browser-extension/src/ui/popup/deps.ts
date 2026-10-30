// What the popup needs from the browser. Adapters build it (adapters/webext/popup.ts).

import type { HandOffAsk } from "../../app/commands.js";
import type { HandOffOutcome } from "../../app/hand-off-site.js";
import type { LoginChoice, SaveOutcome } from "../../domain/login.js";
import type { Result } from "../../domain/result.js";
import type { Tab } from "../../ports/index.js";

export interface PopupDeps {
  /** navigator.language: Korean for "ko*", English otherwise. */
  language: string;
  activeTab(): Promise<Tab | null>;
  /** Asks the person for host access; must run inside the click that started the hand-off. */
  requestHostAccess(origins: string[]): Promise<boolean>;
  /** Asks the background to hand the site off; it checks the ask and answers with counts only. */
  handOff(ask: HandOffAsk): Promise<Result<HandOffOutcome>>;
  /** Saved logins for the tab's site (names only). */
  listLogins(tabId: number): Promise<Result<LoginChoice[]>>;
  /** Fills `item` into the tab after Touch ID; the values never reach the popup. */
  fillLogin(tabId: number, item: string): Promise<Result<{ item: string }>>;
  /** Saves the login typed on the tab as a draft. */
  saveLogin(tabId: number): Promise<Result<SaveOutcome>>;
  loadSession(): Promise<string | null>;
  saveSession(session: string): Promise<void>;
}
