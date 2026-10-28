/** Reads a page's localStorage. */
export interface StorageSource {
  /** The tab's localStorage for its own origin, as string pairs. Rejects if the page cannot be read. */
  readLocalStorage(tabId: number): Promise<Record<string, string>>;
}
