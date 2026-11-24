import { describe, expect, it } from "vitest";
import { formatDuration } from "./format";

describe("formatDuration", () => {
  it("formats minutes and seconds", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(9_999)).toBe("0:09");
    expect(formatDuration(270_000)).toBe("4:30");
  });

  it("adds hours when needed", () => {
    expect(formatDuration(3_723_000)).toBe("1:02:03");
  });

  it("never goes negative", () => {
    expect(formatDuration(-5_000)).toBe("0:00");
  });
});
