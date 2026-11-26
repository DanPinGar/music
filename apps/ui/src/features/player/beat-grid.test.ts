import { describe, expect, it } from "vitest";
import {
  type BeatGrid,
  barBeat,
  barTime,
  beatPosition,
  beatTime,
  gridLines,
  mergeTaps,
  nextBar,
  nextBeat,
  previousBar,
  previousBeat,
  snapTaps,
  tempoAt,
  withDownbeatAt,
} from "./beat-grid";

/** 120 bpm from 1 s, in 4/4, bar 1 on the first beat. */
const STEADY: BeatGrid = { beats: [1, 1.5, 2, 2.5, 3], beatsPerBar: 4, downbeat: 0 };

describe("beat times", () => {
  it("are the tapped beats inside the grid", () => {
    expect(beatTime(STEADY, 2)).toBe(2);
  });

  it("continue at the tempo of the ends outside it", () => {
    expect(beatTime(STEADY, -2)).toBe(0);
    expect(beatTime(STEADY, 6)).toBe(4);
  });

  it("follow a rubato within the tapped beats", () => {
    const rubato: BeatGrid = { beats: [0, 0.5, 1.2, 1.8], beatsPerBar: 4, downbeat: 0 };
    expect(beatPosition(rubato, 0.85)).toBeCloseTo(1.5);
    expect(tempoAt(rubato, 0.6)).toBeCloseTo(60 / 0.7);
  });

  it("invert into positions", () => {
    for (const time of [-0.3, 1, 1.25, 2.9, 7.1]) {
      const position = beatPosition(STEADY, time);
      const index = Math.floor(position);
      const fraction = position - index;
      const back =
        beatTime(STEADY, index) +
        fraction * (beatTime(STEADY, index + 1) - beatTime(STEADY, index));
      expect(back).toBeCloseTo(time);
    }
  });
});

describe("bars and beats", () => {
  it("count from the downbeat", () => {
    expect(barBeat(STEADY, 1)).toEqual({ bar: 1, beat: 1 });
    expect(barBeat(STEADY, 2.6)).toEqual({ bar: 1, beat: 4 });
    expect(barBeat(STEADY, 3.01)).toEqual({ bar: 2, beat: 1 });
  });

  it("number a pickup before the downbeat as bar 0", () => {
    const pickup = { ...STEADY, downbeat: 1 };
    expect(barBeat(pickup, 1.1)).toEqual({ bar: 0, beat: 4 });
    expect(barTime(pickup, 1)).toBe(1.5);
  });

  it("place bars and beats in time", () => {
    expect(barTime(STEADY, 2)).toBe(3);
    expect(barTime(STEADY, 3, 2)).toBe(5.5);
  });
});

describe("stepping", () => {
  it("goes to the next and previous beats, also from a beat", () => {
    expect(nextBeat(STEADY, 1.2)).toBe(1.5);
    expect(nextBeat(STEADY, 1.5)).toBe(2);
    expect(previousBeat(STEADY, 1.7)).toBe(1.5);
    expect(previousBeat(STEADY, 1.5)).toBe(1);
  });

  it("skips the beat just heard while playing", () => {
    expect(previousBeat(STEADY, 1.6, true)).toBe(1);
  });

  it("goes to the next and previous bars", () => {
    expect(nextBar(STEADY, 1)).toBe(3);
    expect(nextBar(STEADY, 2.2)).toBe(3);
    expect(previousBar(STEADY, 3)).toBe(1);
    expect(previousBar(STEADY, 3.4)).toBe(3);
  });
});

describe("gridLines", () => {
  it("lists the beats in view and marks the bars", () => {
    const lines = gridLines(STEADY, 2.4, 5);
    expect(lines.map((line) => line.time)).toEqual([2.5, 3, 3.5, 4, 4.5, 5]);
    expect(lines.map((line) => line.bar)).toEqual([null, 2, null, null, null, 3]);
  });
});

describe("taps", () => {
  it("snap onto nearby attacks", () => {
    expect(snapTaps([1.03, 1.52, 2.2], [1, 1.5, 2])).toEqual([1, 1.5, 2.2]);
  });

  it("never snap two taps onto one attack", () => {
    expect(snapTaps([1.0, 1.04], [1.02])).toEqual([1.02]);
  });

  it("start a grid with the first tap as the downbeat", () => {
    expect(mergeTaps(null, [1, 1.5, 2])).toEqual({
      beats: [1, 1.5, 2],
      beatsPerBar: 4,
      downbeat: 0,
    });
    expect(mergeTaps(null, [1])).toBeNull();
  });

  it("replace the beats of the stretch they cover, keeping the downbeat", () => {
    const grid = { beats: [1, 1.5, 2, 2.5, 3, 3.5, 4], beatsPerBar: 4, downbeat: 2 };
    const merged = mergeTaps(grid, [2.45, 3.05, 3.55]);
    expect(merged?.beats).toEqual([1, 1.5, 2, 2.45, 3.05, 3.55, 4]);
    expect(merged && beatTime(merged, merged.downbeat)).toBe(2);
  });
});

describe("withDownbeatAt", () => {
  it("makes the nearest beat the start of a bar", () => {
    const moved = withDownbeatAt(STEADY, 2.1);
    expect(moved.downbeat).toBe(2);
    expect(barBeat(moved, 2)).toEqual({ bar: 1, beat: 1 });
  });
});
