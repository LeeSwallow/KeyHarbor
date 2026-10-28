import { describe, expect, it } from "vitest";
import { PROTOCOL_VERSION, parseHostReply, toReceiveSiteWire } from "../../../src/adapters/wire/protocol.js";
import type { SiteHandoff } from "../../../src/domain/handoff.js";
import type { SessionName } from "../../../src/domain/session-name.js";
import { SECRET, cookie } from "../../support/fakes.js";

const handoff = (over: Partial<SiteHandoff> = {}): SiteHandoff => ({ origin: "https://github.com", cookies: [], session: "work" as SessionName, ttlSeconds: 3600, ...over });

describe("receive_site", () => {
  it("is version 1 with the documented fields", () => {
    expect(toReceiveSiteWire(handoff())).toEqual({ v: 1, type: "receive_site", origin: "https://github.com", cookies: [], ttl_seconds: 3600, session: "work" });
    expect(PROTOCOL_VERSION).toBe(1);
  });

  it("includes local_storage only when given", () => {
    expect(toReceiveSiteWire(handoff({ origin: "https://a.io", ttlSeconds: 60, localStorage: { k: "v" } })).local_storage).toEqual({ k: "v" });
    expect(toReceiveSiteWire(handoff())).not.toHaveProperty("local_storage");
  });

  it("carries cookies as CDP cookie params", () => {
    expect(toReceiveSiteWire(handoff({ cookies: [cookie()] })).cookies).toEqual([expect.objectContaining({ name: "user_session", value: SECRET, domain: ".github.com", sameSite: "Lax" })]);
  });
});

describe("parseHostReply", () => {
  it("reads the two v1 replies", () => {
    expect(parseHostReply({ v: 1, type: "received", session: "work", cookies: 3 })).toEqual({ ok: true, value: { kind: "received", session: "work", cookies: 3 } });
    expect(parseHostReply({ v: 1, type: "error", code: "no_session" })).toEqual({ ok: true, value: { kind: "refused", code: "no_session" } });
  });

  it.each([null, "ok", [], { v: 2, type: "received", session: "w", cookies: 1 }, { v: 1, type: "received", session: "w", cookies: 1.5 }, { v: 1, type: "error", code: "Has Spaces and a value" }, { v: 1, type: "other" }])(
    "rejects %j",
    (raw) => {
      expect(parseHostReply(raw)).toEqual({ ok: false, error: "bad_reply" });
    },
  );
});
