import { describe, expect, it } from "vitest";
import { buildWaveform, envelope } from "./waveform";

/** 100 pairs per second; pair `i` is `[-i, i]` (clamped to 127). */
function ramp(pairs: number) {
  const peaks: number[] = [];
  for (let i = 0; i < pairs; i++) peaks.push(-Math.min(i, 127), Math.min(i, 127));
  return buildWaveform({ version: 1, peaks_per_second: 100, peaks });
}

describe("buildWaveform", () => {
  it("merges pairs into coarser levels by their extremes", () => {
    const waveform = buildWaveform({
      version: 1,
      peaks_per_second: 100,
      peaks: Array.from({ length: 200 }, (_, i) => (i % 2 === 0 ? -(i % 7) : i % 5)),
    });
    const base = Array.from(waveform.levels[0] ?? []);
    const next = Array.from(waveform.levels[1] ?? []);
    const value = (values: number[], index: number) => values[index] ?? Number.NaN;
    expect(next.length).toBe(100);
    for (let pair = 0; pair < 50; pair++) {
      expect(value(next, 2 * pair)).toBe(
        Math.min(value(base, 4 * pair), value(base, 4 * pair + 2)),
      );
      expect(value(next, 2 * pair + 1)).toBe(
        Math.max(value(base, 4 * pair + 1), value(base, 4 * pair + 3)),
      );
    }
    expect(waveform.duration).toBe(1);
  });

  it("keeps an odd last pair", () => {
    const level = ramp(129).levels[1];
    expect(level?.length).toBe(2 * 65);
    expect(level?.at(-1)).toBe(127);
  });

  it("finds the loudest point, at least 1", () => {
    expect(ramp(50).loudest).toBe(49);
    expect(buildWaveform({ version: 1, peaks_per_second: 100, peaks: [0, 0] }).loudest).toBe(1);
  });
});

describe("envelope", () => {
  it("gives each column the extremes of the audio under it", () => {
    const waveform = ramp(100); // loudest 99
    const out = new Float32Array(2 * 10);
    envelope(waveform, 0, 1, 10, out);
    // Column 3 covers pairs 30–39.
    expect(out[6]).toBeCloseTo(-39 / 99);
    expect(out[7]).toBeCloseTo(39 / 99);
  });

  it("uses coarser levels when columns span many pairs, with the same result", () => {
    const waveform = ramp(1000);
    const coarse = new Float32Array(2 * 4);
    envelope(waveform, 0, 10, 4, coarse);
    // Each column covers 250 pairs; the ramp reaches its clamp (127) within the first.
    expect(coarse[0]).toBeCloseTo(-1);
    expect(coarse[7]).toBeCloseTo(1);
  });

  it("interpolates between pairs when zoomed past one pair per column", () => {
    const waveform = ramp(100);
    const out = new Float32Array(2 * 10);
    // 10 columns over 10 ms, centred at 0.5005 s … 0.5095 s; pair i is centred at (i + 0.5) / 100.
    envelope(waveform, 0.5, 0.51, 10, out);
    const highs = Array.from({ length: 10 }, (_, column) => (out[2 * column + 1] ?? 0) * 99);
    expect(highs[0]).toBeCloseTo(49.55, 2);
    expect(highs[9]).toBeCloseTo(50.45, 2);
    for (let column = 1; column < 10; column++) {
      expect(highs[column]).toBeGreaterThan(highs[column - 1] ?? 0);
    }
  });

  it("is silent outside the recording", () => {
    const out = new Float32Array(2 * 4).fill(9);
    envelope(ramp(100), 5, 6, 4, out);
    expect(Array.from(out)).toEqual(new Array(8).fill(0));
  });
});
