import type { HostReply, SiteHandoff } from "../domain/handoff.js";
import type { Result } from "../domain/result.js";

/** The KeyHarbor native host. */
export interface NativeHost {
  /**
   * Delivers one hand-off and resolves with the host's answer, or `bad_reply` when the answer is
   * not one. Rejects when the host is unavailable. Never logs.
   */
  receiveSite(handoff: SiteHandoff): Promise<Result<HostReply>>;
}
