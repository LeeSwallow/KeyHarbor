import type { LoginValues } from "../domain/login.js";

/** The login form in a tab's top frame. Never logs; never touches other frames. */
export interface PageForm {
  /**
   * Puts `login` into the page's password field (and the username field before it), only if the
   * page is still on `origin`. Resolves `filled`, `no_form`, or `moved` (left `origin`).
   */
  fill(tabId: number, origin: string, login: LoginValues): Promise<"filled" | "no_form" | "moved">;
  /** What the page's login form holds now, or null when it has no filled password field. */
  read(tabId: number, origin: string): Promise<LoginValues | null>;
}
