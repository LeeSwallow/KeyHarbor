import { describe, expect, it } from "vitest";
import { makeAnswerSiteRequest } from "../../src/app/site-request.js";
import { SECRET, fakePorts } from "../support/fakes.js";

describe("an ask from the app to hand a site over", () => {
  it("goes over only after the person presses the browser notification", async () => {
    const ports = fakePorts();
    const answer = await makeAnswerSiteRequest(ports)("https://github.com", "work");
    expect(answer).toEqual({ outcome: "handed_off", cookies: 1 });
    expect(ports.asked).toEqual([["https://github.com", "work"]]);
    expect(ports.sent).toHaveLength(1);
    expect(ports.sent[0]).toMatchObject({ type: "receive_site", origin: "https://github.com", session: "work", ttl_seconds: 3600 });
    expect(JSON.stringify(answer)).not.toContain(SECRET);
  });

  it("sends nothing when the person dismisses the notification", async () => {
    const ports = fakePorts({ pressed: false });
    expect(await makeAnswerSiteRequest(ports)("https://github.com", "work")).toEqual({ outcome: "declined" });
    expect(ports.sent).toEqual([]);
  });

  it("does not ask for a site the extension may not read yet", async () => {
    const ports = fakePorts({ granted: false });
    expect(await makeAnswerSiteRequest(ports)("https://bank.example", "work")).toEqual({ outcome: "no_permission" });
    expect(ports.asked).toEqual([]);
    expect(ports.sent).toEqual([]);
  });

  it("refuses what is not a site or not a session name, before asking", async () => {
    const ports = fakePorts();
    expect(await makeAnswerSiteRequest(ports)("file:///etc/passwd", "work")).toEqual({ outcome: "bad_site" });
    expect(await makeAnswerSiteRequest(ports)("https://github.com", "  ")).toEqual({ outcome: "no_session" });
    expect(ports.asked).toEqual([]);
  });

  it("reports a session the host does not know", async () => {
    const ports = fakePorts({ reply: { v: 1, type: "error", code: "no_session" } });
    expect(await makeAnswerSiteRequest(ports)("https://github.com", "nope")).toEqual({ outcome: "no_session" });
  });
});
