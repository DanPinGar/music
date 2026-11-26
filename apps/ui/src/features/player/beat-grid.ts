import { firstAtOrAfter } from "./attacks";

/**
 * The beats of the song, tapped by the user. Within the tapped stretch each beat is where it
 * was tapped (snapped to the attacks), so a rubato performance keeps its grid; before the first
 * and after the last beat the grid continues at the tempo of its ends.
 */
export interface BeatGrid {
  /** Seconds, strictly increasing; at least two. */
  readonly beats: readonly number[];
  readonly beatsPerBar: number;
  /** Index of a beat that starts bar 1 (it may lie outside `beats`). */
  readonly downbeat: number;
}

export interface BarBeat {
  /** Bar number: 1 from the downbeat, 0 and below before it (a pickup). */
  readonly bar: number;
  /** Beat in the bar, from 1. */
  readonly beat: number;
}

/** The ends' tempo is averaged over up to this many intervals. */
const EDGE_INTERVALS = 4;
/** A tap this close to an attack moves onto it: human taps wobble, attacks do not. */
export const TAP_SNAP_SECONDS = 0.07;
const SAME_TIME = 0.001;
/** While playing, "previous" skips beats heard in the last quarter second. */
const PLAYING_GRACE = 0.25;

function at(values: readonly number[], index: number): number {
  return values[index] ?? 0;
}

function edgePeriods(grid: BeatGrid): { first: number; last: number } {
  const { beats } = grid;
  const n = beats.length;
  const k = Math.min(EDGE_INTERVALS, n - 1);
  return {
    first: (at(beats, k) - at(beats, 0)) / k,
    last: (at(beats, n - 1) - at(beats, n - 1 - k)) / k,
  };
}

/** Time of beat `index` (any integer: beyond the tapped beats the grid is extrapolated). */
export function beatTime(grid: BeatGrid, index: number): number {
  const { beats } = grid;
  const n = beats.length;
  const { first, last } = edgePeriods(grid);
  if (index < 0) return at(beats, 0) + index * first;
  if (index >= n) return at(beats, n - 1) + (index - n + 1) * last;
  return at(beats, index);
}

/** Where `time` falls in beats: 3.5 is halfway between beats 3 and 4. */
export function beatPosition(grid: BeatGrid, time: number): number {
  const { beats } = grid;
  const n = beats.length;
  const { first, last } = edgePeriods(grid);
  if (time < at(beats, 0)) return (time - at(beats, 0)) / first;
  if (time >= at(beats, n - 1)) return n - 1 + (time - at(beats, n - 1)) / last;
  const next = firstAtOrAfter(beats, time);
  if (at(beats, next) === time) return next;
  const before = next - 1;
  return before + (time - at(beats, before)) / (at(beats, next) - at(beats, before));
}

/** Bar and beat at `time` (the beat that has started). */
export function barBeat(grid: BeatGrid, time: number): BarBeat {
  const index = Math.floor(beatPosition(grid, time) + 1e-6);
  const relative = index - grid.downbeat;
  const bar = Math.floor(relative / grid.beatsPerBar) + 1;
  return { bar, beat: relative - (bar - 1) * grid.beatsPerBar + 1 };
}

/** When beat `beat` of bar `bar` starts. */
export function barTime(grid: BeatGrid, bar: number, beat = 1): number {
  return beatTime(grid, grid.downbeat + (bar - 1) * grid.beatsPerBar + (beat - 1));
}

export function nextBeat(grid: BeatGrid, time: number): number {
  return beatTime(grid, Math.floor(beatPosition(grid, time + SAME_TIME)) + 1);
}

export function previousBeat(grid: BeatGrid, time: number, playing = false): number {
  const limit = time - (playing ? PLAYING_GRACE : SAME_TIME);
  return beatTime(grid, Math.ceil(beatPosition(grid, limit)) - 1);
}

export function nextBar(grid: BeatGrid, time: number): number {
  const relative = beatPosition(grid, time + SAME_TIME) - grid.downbeat;
  const bars = Math.floor(relative / grid.beatsPerBar + 1e-9) + 1;
  return beatTime(grid, grid.downbeat + bars * grid.beatsPerBar);
}

export function previousBar(grid: BeatGrid, time: number, playing = false): number {
  const limit = time - (playing ? PLAYING_GRACE : SAME_TIME);
  const relative = beatPosition(grid, limit) - grid.downbeat;
  const bars = Math.ceil(relative / grid.beatsPerBar - 1e-9) - 1;
  return beatTime(grid, grid.downbeat + bars * grid.beatsPerBar);
}

export interface GridLine {
  readonly time: number;
  /** The first beat of a bar. */
  readonly bar: number | null;
}

/** The beats between `start` and `end`, marking where bars start (at most `limit`). */
export function gridLines(grid: BeatGrid, start: number, end: number, limit = 4000): GridLine[] {
  const lines: GridLine[] = [];
  const first = Math.ceil(beatPosition(grid, start) - 1e-9);
  const last = Math.floor(beatPosition(grid, end) + 1e-9);
  for (let index = first; index <= last && lines.length < limit; index++) {
    const relative = index - grid.downbeat;
    const isBar = ((relative % grid.beatsPerBar) + grid.beatsPerBar) % grid.beatsPerBar === 0;
    lines.push({
      time: beatTime(grid, index),
      bar: isBar ? Math.floor(relative / grid.beatsPerBar) + 1 : null,
    });
  }
  return lines;
}

/** Beats per minute around `time`. */
export function tempoAt(grid: BeatGrid, time: number): number {
  const index = Math.floor(beatPosition(grid, time));
  return 60 / (beatTime(grid, index + 1) - beatTime(grid, index));
}

/** Each tap moved onto the nearest attack within `TAP_SNAP_SECONDS`, keeping them in order. */
export function snapTaps(taps: readonly number[], attacks: readonly number[] | null): number[] {
  const snapped: number[] = [];
  for (const tap of taps) {
    let best = tap;
    let distance = TAP_SNAP_SECONDS;
    if (attacks && attacks.length > 0) {
      const after = firstAtOrAfter(attacks, tap);
      for (const candidate of [attacks[after - 1], attacks[after]]) {
        if (candidate !== undefined && Math.abs(candidate - tap) <= distance) {
          best = candidate;
          distance = Math.abs(candidate - tap);
        }
      }
    }
    if (snapped.length === 0 || best > (snapped.at(-1) ?? 0) + SAME_TIME) snapped.push(best);
  }
  return snapped;
}

/**
 * The grid after a run of taps: the taps replace the beats of the stretch they cover (half a
 * beat around it). A first run starts the grid, with its first tap as the downbeat; later runs
 * keep the downbeat where it was.
 */
export function mergeTaps(grid: BeatGrid | null, taps: readonly number[]): BeatGrid | null {
  if (taps.length < 2) return grid;
  const firstTap = at(taps, 0);
  const lastTap = at(taps, taps.length - 1);
  if (!grid) return { beats: [...taps], beatsPerBar: 4, downbeat: 0 };
  const half = (lastTap - firstTap) / (taps.length - 1) / 2;
  const kept = grid.beats.filter((time) => time < firstTap - half || time > lastTap + half);
  const beats = [...kept, ...taps].sort((a, b) => a - b);
  const merged = { ...grid, beats };
  const downbeatTime = beatTime(grid, grid.downbeat);
  return { ...merged, downbeat: Math.round(beatPosition(merged, downbeatTime)) };
}

/** The grid with the beat nearest to `time` as the start of a bar. */
export function withDownbeatAt(grid: BeatGrid, time: number): BeatGrid {
  return { ...grid, downbeat: Math.round(beatPosition(grid, time)) };
}

export function withBeatsPerBar(grid: BeatGrid, beatsPerBar: number): BeatGrid {
  return { ...grid, beatsPerBar };
}
