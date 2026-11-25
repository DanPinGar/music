/** Default step sizes and limits of the study player (see docs/PLAN.md §4.2). */
export const STEP_SECONDS = 2;
export const LONG_STEP_SECONDS = 10;
export const REPLAY_SECONDS = 3;
export const MIN_RATE = 0.25;
export const MAX_RATE = 2;
export const RATE_STEP = 0.05;

/** Keeps a time inside the media (0 … duration, when the duration is known). */
export function clampTime(time: number, duration: number): number {
  const end = Number.isFinite(duration) && duration > 0 ? duration : Number.POSITIVE_INFINITY;
  return Math.min(Math.max(time, 0), end);
}

/**
 * The time to seek to for showing the frame `frames` away from the one at `time`. It aims
 * at the middle of the target frame: seeking to a frame boundary may show the frame before.
 */
export function frameStep(time: number, fps: number, frames: number): number {
  const current = Math.floor(time * fps + 1e-6);
  return Math.max(0, (current + frames + 0.5) / fps);
}

/** Playback rate after a change, kept within limits and rounded to whole percents. */
export function nextRate(rate: number, delta: number): number {
  const clamped = Math.min(MAX_RATE, Math.max(MIN_RATE, rate + delta));
  return Math.round(clamped * 100) / 100;
}

/** "1:02", "1:02.345" with milliseconds, or "1:01:02" past an hour. */
export function formatTimecode(seconds: number, withMillis = false): string {
  const safe = Number.isFinite(seconds) && seconds > 0 ? seconds : 0;
  const totalMillis = Math.floor(safe * 1000 + 1e-6);
  const millis = totalMillis % 1000;
  const totalSeconds = Math.floor(totalMillis / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const secs = String(totalSeconds % 60).padStart(2, "0");
  const base =
    hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${secs}` : `${minutes}:${secs}`;
  return withMillis ? `${base}.${String(millis).padStart(3, "0")}` : base;
}
