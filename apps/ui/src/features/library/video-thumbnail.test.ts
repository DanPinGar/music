import { describe, expect, it } from "vitest";
import { tilePosition } from "./video-thumbnail";

describe("tilePosition", () => {
  it("places tiles of a 10-column sprite", () => {
    expect(tilePosition(0, 10, 15)).toBe("0% 0%");
    expect(tilePosition(9, 10, 15)).toBe("100% 0%");
    expect(tilePosition(14, 10, 15)).toBe(`${(4 / 9) * 100}% ${(1 / 14) * 100}%`);
    expect(tilePosition(141, 10, 15)).toBe(`${(1 / 9) * 100}% 100%`);
  });

  it("handles sprites with a single row or column", () => {
    expect(tilePosition(3, 10, 1)).toBe(`${(3 / 9) * 100}% 0%`);
    expect(tilePosition(2, 1, 5)).toBe(`0% 50%`);
  });
});
