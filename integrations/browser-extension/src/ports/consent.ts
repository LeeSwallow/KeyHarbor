/** What the extension may already do, without asking the person again. */
export interface Permissions {
  /** Whether host access for all of `origins` was granted before. Never prompts. */
  has(origins: string[]): Promise<boolean>;
}

/** Asks the person, in their browser, before something they did not start here goes ahead. */
export interface Notifier {
  /**
   * Shows "hand `origin` over to the session `session`?" and resolves true only when the person
   * presses it; false when they dismiss it or it times out.
   */
  askHandOff(origin: string, session: string): Promise<boolean>;
}
