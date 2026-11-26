/** The stretch of the song the detail timeline shows, in seconds. */
export interface View {
  readonly start: number;
  readonly span: number;
}

/** How the detail timeline follows playback: page by page, scrolling with it, or not at all. */
export type FollowMode = "page" | "continuous" | "off";
export const FOLLOW_MODES: readonly FollowMode[] = ["page", "continuous", "off"];

/** Half a second: 30 frames of a 60 fps video across the timeline. */
export const MIN_SPAN = 0.5;
export const DEFAULT_SPAN = 10;
/** Paging leaves this share of the old page visible at the left of the new one. */
const PAGE_OVERLAP = 0.05;

/** Keeps the span between `MIN_SPAN` and the song, and the view inside the song. */
export function clampView(view: View, duration: number): View {
  if (!(duration > 0)) return { start: 0, span: Math.max(view.span, MIN_SPAN) };
  const span = Math.min(Math.max(view.span, Math.min(MIN_SPAN, duration)), duration);
  const start = Math.min(Math.max(view.start, 0), duration - span);
  return { start, span };
}

/** The whole song. */
export function fitView(duration: number): View {
  return clampView({ start: 0, span: duration }, duration);
}

/** Zooms by `factor` (below 1 zooms in) keeping the time `anchor` at the same place. */
export function zoomView(view: View, factor: number, anchor: number, duration: number): View {
  const span = clampView({ start: 0, span: view.span * factor }, duration).span;
  const start = anchor - ((anchor - view.start) * span) / view.span;
  return clampView({ start, span }, duration);
}

export function panView(view: View, seconds: number, duration: number): View {
  return clampView({ start: view.start + seconds, span: view.span }, duration);
}

/**
 * The view after playback reached `time`: a new page once the playhead leaves the old one,
 * or centred on it when scrolling continuously.
 */
export function followView(view: View, time: number, mode: FollowMode, duration: number): View {
  if (mode === "continuous") {
    return clampView({ start: time - view.span / 2, span: view.span }, duration);
  }
  if (mode === "page" && (time < view.start || time > view.start + view.span)) {
    return clampView({ start: time - view.span * PAGE_OVERLAP, span: view.span }, duration);
  }
  return view;
}

/** The view after a jump to `time`: jumps always bring the playhead into view. */
export function revealView(view: View, time: number, mode: FollowMode, duration: number): View {
  return followView(view, time, mode === "off" ? "page" : mode, duration);
}

export function timeAt(view: View, x: number, width: number): number {
  return view.start + (x / width) * view.span;
}

export function xAt(view: View, time: number, width: number): number {
  return ((time - view.start) / view.span) * width;
}

/** Ruler steps in seconds, each with the number of minor divisions between labels. */
const STEPS: readonly (readonly [number, number])[] = [
  [0.01, 5],
  [0.02, 4],
  [0.05, 5],
  [0.1, 5],
  [0.2, 4],
  [0.5, 5],
  [1, 5],
  [2, 4],
  [5, 5],
  [10, 5],
  [15, 3],
  [30, 6],
  [60, 6],
  [120, 4],
  [300, 5],
  [600, 5],
  [1200, 4],
  [1800, 6],
  [3600, 6],
];
const COARSEST_STEP = [3600, 6] as const;

export interface Ruler {
  /** Seconds between labelled ticks. */
  readonly step: number;
  /** Seconds between minor ticks. */
  readonly minor: number;
  /** Times of the labelled ticks in view. */
  readonly labels: readonly number[];
}

/** The finest ruler whose labels stay `minSpacing` pixels apart. */
export function ruler(view: View, width: number, minSpacing: number): Ruler {
  const pixelsPerSecond = width / view.span;
  const [step, divisions] =
    STEPS.find(([candidate]) => candidate * pixelsPerSecond >= minSpacing) ?? COARSEST_STEP;
  const labels: number[] = [];
  const first = Math.ceil(view.start / step - 1e-9);
  for (let index = first; index * step <= view.start + view.span + 1e-9; index++) {
    labels.push(roundTo(index * step, step));
  }
  return { step, minor: step / divisions, labels };
}

/** A ruler label: "1:05" for whole seconds, "1:05.5" or "1:05.25" below. */
export function rulerLabel(time: number, step: number): string {
  const decimals = step >= 1 ? 0 : step >= 0.1 ? 1 : 2;
  const totalSeconds = Math.floor(time + 1e-9);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = String(totalSeconds % 60).padStart(2, "0");
  if (decimals === 0) return `${minutes}:${seconds}`;
  const fraction = (time - totalSeconds).toFixed(decimals).slice(1);
  return `${minutes}:${seconds}${fraction}`;
}

function roundTo(value: number, step: number): number {
  const decimals = Math.max(0, -Math.floor(Math.log10(step)) + 1);
  return Number(value.toFixed(decimals));
}
