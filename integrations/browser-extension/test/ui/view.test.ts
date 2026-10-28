import { describe, expect, it } from "vitest";
import { ERRORS, MESSAGES, format, pickLanguage } from "../../src/ui/popup/i18n.js";
import { escape, renderPopup } from "../../src/ui/popup/view.js";

const ready = { kind: "ready" as const, origin: "https://github.com", session: "work", ttlSeconds: 3600, includeLocalStorage: false, busy: false };

describe("i18n", () => {
  it("has every message and error in Korean and English", () => {
    expect(Object.keys(MESSAGES.en).sort()).toEqual(Object.keys(MESSAGES.ko).sort());
    expect(Object.keys(ERRORS.en).sort()).toEqual(Object.keys(ERRORS.ko).sort());
  });

  it("picks Korean for ko* and English otherwise", () => {
    expect(pickLanguage("ko-KR")).toBe("ko");
    expect(pickLanguage("en-US")).toBe("en");
    expect(pickLanguage("ja")).toBe("en");
  });

  it("fills placeholders", () => {
    expect(format("{n} → {session}", { n: 3, session: "w" })).toBe("3 → w");
  });
});

describe("renderPopup", () => {
  it("builds the popup from design-system classes", () => {
    const html = renderPopup(ready, "ko");
    for (const cls of ["kh-dialog", "kh-origins", "kh-input", "kh-seg", "kh-toggle", "kh-btn primary", "kh-sealbar"]) expect(html).toContain(cls);
    expect(html).toContain("design/icons/hand.svg");
    expect(html).toContain('data-ttl="3600" class="is-on"');
  });

  it("escapes what it shows", () => {
    expect(escape(`<a href="x">&'`)).toBe("&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    expect(renderPopup({ ...ready, session: '"><img src=x>' }, "en")).not.toContain("<img");
  });

  it("shows errors as text for their code", () => {
    expect(renderPopup({ ...ready, error: "host_unavailable" }, "en")).toContain("keyharbor browser install-host");
  });

  it("says when a page cannot be handed off", () => {
    expect(renderPopup({ kind: "unsupported" }, "en")).toContain("kh-empty");
  });
});
