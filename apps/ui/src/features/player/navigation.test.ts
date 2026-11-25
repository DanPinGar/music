import { describe, expect, it } from "vitest";
import { clampTime, formatTimecode, frameStep, nextRate } from "./navigation";

describe("frameStep", () => {
  it("lands in the middle of the next and previous frames", () => {
    expect(frameStep(0, 60, 1)).toBeCloseTo(1.5 / 60);
    const atFrame10 = 10.5 / 60;
    expect(frameStep(atFrame10, 60, 1)).toBeCloseTo(11.5 / 60);
    expect(frameStep(atFrame10, 60, -1)).toBeCloseTo(9.5 / 60);
  });

  it("is stable when called repeatedly", () => {
    let time = 0;
    for (let i = 0; i < 120; i += 1) time = frameStep(time, 60, 1);
    expect(Math.floor(time * 60)).toBe(120);
  });

  it("never goes before the start", () => {
    expect(frameStep(0, 30, -1)).toBe(0);
  });
});

describe("nextRate", () => {
  it("moves in steps and stays within 25 % and 200 %", () => {
    expect(nextRate(1, -0.05)).toBe(0.95);
    expect(nextRate(0.25, -0.05)).toBe(0.25);
    expect(nextRate(2, 0.05)).toBe(2);
  });

  it("does not accumulate floating-point noise", () => {
    let rate = 1;
    for (let i = 0; i < 7; i += 1) rate = nextRate(rate, -0.05);
    expect(rate).toBe(0.65);
  });
});

describe("clampTime", () => {
  it("keeps times inside the media", () => {
    expect(clampTime(-3, 100)).toBe(0);
    expect(clampTime(150, 100)).toBe(100);
    expect(clampTime(150, Number.NaN)).toBe(150);
  });
});

describe("formatTimecode", () => {
  it("formats minutes, seconds and milliseconds", () => {
    expect(formatTimecode(0)).toBe("0:00");
    expect(formatTimecode(141.154)).toBe("2:21");
    expect(formatTimecode(72.5, true)).toBe("1:12.500");
    expect(formatTimecode(3_723.04, true)).toBe("1:02:03.040");
  });

  it("treats unknown times as zero", () => {
    expect(formatTimecode(Number.NaN, true)).toBe("0:00.000");
  });
});
