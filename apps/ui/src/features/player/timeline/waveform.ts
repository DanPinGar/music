import type { Peaks } from "@segno/api-client";

/** The audio envelope at several resolutions, so drawing costs the same at any zoom. */
export interface Waveform {
  /** Pairs per second at the finest level. */
  readonly rate: number;
  /** `levels[k]` holds `[min, max, min, max, …]`, each pair merging 2^k pairs of level 0. */
  readonly levels: readonly Int8Array[];
  /** Seconds of audio covered. */
  readonly duration: number;
  /** Largest absolute value (at least 1): the drawing scales it to the full height. */
  readonly loudest: number;
}

/** Coarser levels stop at about this many pairs. */
const COARSEST_PAIRS = 64;
const EMPTY = new Int8Array(0);

/** The value at `index`, or 0 past the end (indexes are always in range here). */
function at(values: Int8Array, index: number): number {
  return values[index] ?? 0;
}

export function buildWaveform(peaks: Peaks): Waveform {
  const base = Int8Array.from(peaks.peaks);
  let loudest = 1;
  for (const value of base) loudest = Math.max(loudest, Math.abs(value));
  const levels: Int8Array[] = [base];
  let current = base;
  while (current.length / 2 > COARSEST_PAIRS) {
    const pairs = current.length / 2;
    const next = new Int8Array(Math.ceil(pairs / 2) * 2);
    for (let pair = 0; pair < next.length / 2; pair++) {
      const first = 4 * pair;
      const second = first + 2 < current.length ? first + 2 : first;
      next[2 * pair] = Math.min(at(current, first), at(current, second));
      next[2 * pair + 1] = Math.max(at(current, first + 1), at(current, second + 1));
    }
    levels.push(next);
    current = next;
  }
  const rate = peaks.peaks_per_second;
  return { rate, levels, duration: base.length / 2 / rate, loudest };
}

/**
 * The envelope between `start` and `end` seconds in `columns` equal slices, written to `out`
 * as `[min, max, …]` from -1 to 1 (1 being the loudest point of the recording). Slices
 * narrower than a pair are interpolated between pair centres, so deep zooms stay smooth.
 */
export function envelope(
  waveform: Waveform,
  start: number,
  end: number,
  columns: number,
  out: Float32Array,
): void {
  const { rate, levels, loudest } = waveform;
  const perColumn = (end - start) / columns;
  const scale = 1 / loudest;
  const pairsPerColumn = perColumn * rate;

  if (pairsPerColumn < 1) {
    const base = levels[0] ?? EMPTY;
    const pairs = base.length / 2;
    for (let column = 0; column < columns; column++) {
      const position = (start + (column + 0.5) * perColumn) * rate - 0.5;
      if (position < -0.5 || position > pairs - 0.5 || pairs === 0) {
        out[2 * column] = 0;
        out[2 * column + 1] = 0;
        continue;
      }
      const left = Math.max(0, Math.min(pairs - 1, Math.floor(position)));
      const right = Math.min(pairs - 1, left + 1);
      const fraction = Math.max(0, Math.min(1, position - left));
      out[2 * column] = lerp(at(base, 2 * left), at(base, 2 * right), fraction) * scale;
      out[2 * column + 1] = lerp(at(base, 2 * left + 1), at(base, 2 * right + 1), fraction) * scale;
    }
    return;
  }

  const level = Math.min(levels.length - 1, Math.floor(Math.log2(pairsPerColumn)));
  const values = levels[level] ?? EMPTY;
  const pairs = values.length / 2;
  const levelRate = rate / 2 ** level;
  for (let column = 0; column < columns; column++) {
    const from = Math.max(0, Math.floor((start + column * perColumn) * levelRate));
    const to = Math.min(
      pairs,
      Math.max(from + 1, Math.ceil((start + (column + 1) * perColumn) * levelRate)),
    );
    let low = 0;
    let high = 0;
    for (let pair = from; pair < to; pair++) {
      low = Math.min(low, at(values, 2 * pair));
      high = Math.max(high, at(values, 2 * pair + 1));
    }
    out[2 * column] = low * scale;
    out[2 * column + 1] = high * scale;
  }
}

function lerp(a: number, b: number, fraction: number): number {
  return a + (b - a) * fraction;
}
