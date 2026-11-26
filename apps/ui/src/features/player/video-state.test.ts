import { describe, expect, it } from "vitest";
import { NO_PRACTICE } from "./session";
import { fromApi, type StudyState, toApi } from "./video-state";

const STATE: StudyState = {
  position: 30.2,
  speed: 0.75,
  stepUnit: "bar",
  view: { start: 28, span: 10 },
  grid: { beats: [0.5, 1.1, 1.7], beatsPerBar: 3, downbeat: 0 },
  loop: { start: 1, end: 9, enabled: true },
  practice: { preRollBeats: 2, countIn: true, trainer: { from: 0.6, step: 0.05, to: 1 } },
};

describe("video state", () => {
  it("survives the round trip through the API", () => {
    expect(fromApi(toApi(STATE))).toEqual(STATE);
  });

  it("drops beats that fall on the same millisecond", () => {
    const state = { ...STATE, grid: { beats: [1, 1.0001, 2], beatsPerBar: 4, downbeat: 0 } };
    expect(toApi(state).beat_grid?.beats_ms).toEqual([1000, 2000]);
  });

  it("leaves out a grid or loop the core would refuse", () => {
    const state = {
      ...STATE,
      grid: { beats: [1, 1.0001], beatsPerBar: 4, downbeat: 0 },
      loop: { start: 5, end: 5.0001, enabled: true },
    };
    const api = toApi(state);
    expect(api.beat_grid).toBeNull();
    expect(api.loop).toBeNull();
  });

  it("reads the defaults of a video never opened", () => {
    const state = fromApi({
      position_ms: 0,
      speed: 1,
      step_unit: "seconds",
      practice: { pre_roll_beats: 0, count_in: false },
    });
    expect(state).toEqual({
      position: 0,
      speed: 1,
      stepUnit: "seconds",
      view: null,
      grid: null,
      loop: null,
      practice: NO_PRACTICE,
    });
  });
});
