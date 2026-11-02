import { describe, expect, it } from "vitest";
import { storeManifest } from "../../scripts/package.mjs";

describe("store packages", () => {
  it("package_drops_the_key_field", () => {
    const manifest = { manifest_version: 3, name: "KeyHarbor", key: "MIIB…", version: "0.2.0" };
    const store = storeManifest(manifest);
    expect(store).not.toHaveProperty("key");
    expect(store).toEqual({ manifest_version: 3, name: "KeyHarbor", version: "0.2.0" });
    expect(manifest.key).toBe("MIIB…");
  });
});
