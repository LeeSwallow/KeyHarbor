import { describe, expect, it } from "vitest";
import { TTL_CHOICES, TTL_RANGE, handOffRequest } from "../../src/domain/handoff.js";

describe("handOffRequest", () => {
  it("accepts a session name and a TTL in range", () => {
    expect(handOffRequest("work-2", 3600, true)).toEqual({ ok: true, value: { session: "work-2", ttlSeconds: 3600, includeLocalStorage: true } });
  });

  it.each(["", "Work", "-x", "a b", "a".repeat(65), "../etc"])("rejects session %j", (s) => {
    expect(handOffRequest(s, 3600, false)).toEqual({ ok: false, error: "bad_session" });
  });

  it.each([0, 59, TTL_RANGE.max + 1, 1.5, Number.NaN])("rejects ttl %s", (ttl) => {
    expect(handOffRequest("work", ttl, false)).toEqual({ ok: false, error: "bad_ttl" });
  });

  it("offers only TTLs inside the range", () => {
    for (const ttl of TTL_CHOICES) expect(handOffRequest("w", ttl, false).ok).toBe(true);
  });
});
