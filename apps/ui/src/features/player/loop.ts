import { firstAtOrAfter } from "./attacks";

/** The stretch played over and over (A-B). */
export interface Loop {
  readonly start: number;
  readonly end: number;
  readonly enabled: boolean;
}

/** Speeds the loop up on every repetition: `from`, then `step` more each time, up to `to`. */
export interface SpeedTrainer {
  readonly from: number;
  readonly step: number;
  readonly to: number;
}

export const DEFAULT_TRAINER: SpeedTrainer = { from: 0.6, step: 0.05, to: 1 };
/** A loop is at least this long. */
export const MIN_LOOP_SECONDS = 0.1;

/**
 * The loop after setting its start (A) at `time`. Without a valid end yet, it gets one
 * `length` later, ready for B to adjust.
 */
export function withStart(loop: Loop | null, time: number, length: number, duration: number): Loop {
  const end =
    loop && loop.end > time + MIN_LOOP_SECONDS ? loop.end : Math.min(duration, time + length);
  return { start: time, end: Math.max(end, time + MIN_LOOP_SECONDS), enabled: true };
}

/** The loop after setting its end (B) at `time`; without a valid start, one `length` earlier. */
export function withEnd(loop: Loop | null, time: number, length: number): Loop {
  const start =
    loop && loop.start < time - MIN_LOOP_SECONDS ? loop.start : Math.max(0, time - length);
  return { start: Math.min(start, time - MIN_LOOP_SECONDS), end: time, enabled: true };
}

/**
 * Whether playback must wrap round to the loop start: it was inside the loop (or its
 * pre-roll) and has reached the end. Playing on from outside the loop is never stopped.
 */
export function mustWrap(loop: Loop, previous: number, time: number, preRoll: number): boolean {
  if (!loop.enabled) return false;
  const wasInside = previous >= loop.start - preRoll - 0.05 && previous < loop.end;
  return wasInside && time >= loop.end;
}

/** The speed for repetition number `repetition` (0 is the first pass). */
export function trainerSpeed(trainer: SpeedTrainer, repetition: number): number {
  return Math.round(Math.min(trainer.to, trainer.from + trainer.step * repetition) * 100) / 100;
}

/**
 * `time` moved onto the nearest of `candidates` (in order) within `radius` seconds, or left
 * where it is.
 */
export function snapTo(time: number, candidates: readonly number[], radius: number): number {
  const after = firstAtOrAfter(candidates, time);
  let best = time;
  let distance = radius;
  for (const candidate of [candidates[after - 1], candidates[after]]) {
    if (candidate !== undefined && Math.abs(candidate - time) <= distance) {
      best = candidate;
      distance = Math.abs(candidate - time);
    }
  }
  return best;
}
