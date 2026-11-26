import type { VideoState } from "@segno/api-client";
import type { BeatGrid } from "./beat-grid";
import type { Loop } from "./loop";
import { NO_PRACTICE, type Practice, type StepUnit } from "./session";
import type { View } from "./timeline/view";

/** Everything a video reopens with, in the UI's units (seconds). */
export interface StudyState {
  readonly position: number;
  readonly speed: number;
  readonly stepUnit: StepUnit;
  readonly view: View | null;
  readonly grid: BeatGrid | null;
  readonly loop: Loop | null;
  readonly practice: Practice;
}

const ms = (seconds: number) => Math.round(seconds * 1000);

export function fromApi(state: VideoState): StudyState {
  return {
    position: state.position_ms / 1000,
    speed: state.speed,
    stepUnit: state.step_unit,
    view: state.view
      ? { start: state.view.start_ms / 1000, span: state.view.span_ms / 1000 }
      : null,
    grid: state.beat_grid
      ? {
          beats: state.beat_grid.beats_ms.map((time) => time / 1000),
          beatsPerBar: state.beat_grid.beats_per_bar,
          downbeat: state.beat_grid.downbeat,
        }
      : null,
    loop: state.loop
      ? {
          start: state.loop.start_ms / 1000,
          end: state.loop.end_ms / 1000,
          enabled: state.loop.enabled,
        }
      : null,
    practice: state.practice
      ? {
          preRollBeats: state.practice.pre_roll_beats,
          countIn: state.practice.count_in,
          trainer: state.practice.trainer ?? null,
        }
      : NO_PRACTICE,
  };
}

export function toApi(state: StudyState): VideoState {
  // Beats can merge to the same millisecond; the core wants them strictly increasing.
  const beats: number[] = [];
  for (const time of state.grid?.beats ?? []) {
    const value = ms(time);
    if (beats.length === 0 || value > (beats.at(-1) ?? 0)) beats.push(value);
  }
  return {
    position_ms: Math.max(0, ms(state.position)),
    speed: state.speed,
    step_unit: state.stepUnit,
    view: state.view
      ? { start_ms: Math.max(0, ms(state.view.start)), span_ms: Math.max(100, ms(state.view.span)) }
      : null,
    beat_grid:
      state.grid && beats.length >= 2
        ? {
            beats_ms: beats,
            beats_per_bar: state.grid.beatsPerBar,
            downbeat: state.grid.downbeat,
          }
        : null,
    loop:
      state.loop && ms(state.loop.end) > ms(state.loop.start)
        ? {
            start_ms: Math.max(0, ms(state.loop.start)),
            end_ms: ms(state.loop.end),
            enabled: state.loop.enabled,
          }
        : null,
    practice: {
      pre_roll_beats: state.practice.preRollBeats,
      count_in: state.practice.countIn,
      trainer: state.practice.trainer,
    },
  };
}
