/** A browser tab, as far as the hand-off needs it. */
export interface Tab {
  id: number;
  url: string;
}

/** Looks up tabs. */
export interface TabInfo {
  /** The focused tab of the current window, or null. */
  active(): Promise<Tab | null>;
  /** A tab by id, or null when it is gone. */
  find(tabId: number): Promise<Tab | null>;
}
