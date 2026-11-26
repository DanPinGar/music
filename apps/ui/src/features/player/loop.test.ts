import { describe, expect, it } from "vitest";
import { mustWrap, snapTo, trainerSpeed, withEnd, withStart } from "./loop";

describe("setting A and B", () => {
  it("A keeps a valid end, or makes one a length later", () => {
    expect(withStart({ start: 10, end: 20, enabled: false }, 12, 4, 100)).toEqual({
      start: 12,
      end: 20,
      enabled: true,
    });
    expect(withStart(null, 12, 4, 100)).toEqual({ start: 12, end: 16, enabled: true });
    expect(withStart({ start: 10, end: 20, enabled: true }, 25, 4, 100).end).toBe(29);
    expect(withStart(null, 98, 4, 100).end).toBe(100);
  });

  it("B keeps a valid start, or makes one a length earlier", () => {
    expect(withEnd({ start: 10, end: 20, enabled: false }, 15, 4)).toEqual({
      start: 10,
      end: 15,
      enabled: true,
    });
    expect(withEnd(null, 3, 4)).toEqual({ start: 0, end: 3, enabled: true });
    expect(withEnd({ start: 10, end: 20, enabled: true }, 8, 4).start).toBe(4);
  });
});

describe("mustWrap", () => {
  const loop = { start: 10, end: 20, enabled: true };

  it("wraps when playback inside the loop reaches its end", () => {
    expect(mustWrap(loop, 19.99, 20.01, 0)).toBe(true);
    expect(mustWrap(loop, 15, 16, 0)).toBe(false);
  });

  it("counts the pre-roll as inside", () => {
    expect(mustWrap(loop, 8.5, 20, 2)).toBe(true);
  });

  it("lets playback from outside go on", () => {
    expect(mustWrap(loop, 25, 25.1, 0)).toBe(false);
    expect(mustWrap({ ...loop, enabled: false }, 19.99, 20.01, 0)).toBe(false);
  });
});

describe("trainerSpeed", () => {
  it("rises by a step each repetition up to the top", () => {
    const trainer = { from: 0.6, step: 0.05, to: 0.7 };
    expect([0, 1, 2, 3].map((repetition) => trainerSpeed(trainer, repetition))).toEqual([
      0.6, 0.65, 0.7, 0.7,
    ]);
  });
});

describe("snapTo", () => {
  it("moves onto the nearest candidate within the radius", () => {
    expect(snapTo(10.07, [9, 10, 10.1], 0.08)).toBe(10.1);
    expect(snapTo(10.03, [9, 10, 10.1], 0.08)).toBe(10);
    expect(snapTo(10.5, [9, 10, 11], 0.08)).toBe(10.5);
  });
});
