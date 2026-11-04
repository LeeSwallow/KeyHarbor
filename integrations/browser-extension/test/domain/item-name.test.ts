import { describe, expect, it } from "vitest";
import { isFillableItemName } from "../../src/domain/item-name.js";

describe("isFillableItemName", () => {
  it.each(["github", "myapp/prod/db", "깃허브_1", "a-b", "x".repeat(64)])("accepts %j", (name) => {
    expect(isFillableItemName(name)).toBe(true);
  });

  it.each(["", "x".repeat(65), "github~draft-1", "a.b", "a b", "a\nb"])("rejects %j", (name) => {
    expect(isFillableItemName(name)).toBe(false);
  });
});
