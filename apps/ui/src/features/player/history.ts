/** Jumps shorter than this are steps, not navigation (arrows, frames, beats). */
const MIN_JUMP_SECONDS = 4;
/** Seeks this close together (scrubbing, a held key) are one navigation. */
const BURST_MS = 800;
const MAX_ENTRIES = 100;

/**
 * Where the user jumped from, as in a browser: back returns to the place before the last
 * jump, forward undoes it. Fed with every seek; it keeps only real jumps, and one entry per
 * burst of seeks.
 */
export class NavigationHistory {
  #back: number[] = [];
  #forward: number[] = [];
  #lastSeekAt = Number.NEGATIVE_INFINITY;
  #burstFrom = 0;
  #burstRecorded = false;

  /** A seek from `from` to `to` happened at `now` (milliseconds, any clock). */
  noteSeek(from: number, to: number, now: number): void {
    if (now - this.#lastSeekAt > BURST_MS) {
      this.#burstFrom = from;
      this.#burstRecorded = false;
    }
    this.#lastSeekAt = now;
    if (!this.#burstRecorded && Math.abs(to - this.#burstFrom) >= MIN_JUMP_SECONDS) {
      this.#back.push(this.#burstFrom);
      if (this.#back.length > MAX_ENTRIES) this.#back.shift();
      this.#forward = [];
      this.#burstRecorded = true;
    }
  }

  /** Where to go back to from `current`, if anywhere. */
  back(current: number): number | undefined {
    const target = this.#back.pop();
    if (target !== undefined) this.#forward.push(current);
    this.#lastSeekAt = Number.NEGATIVE_INFINITY;
    return target;
  }

  /** Where to go forward to from `current`, if anywhere. */
  forward(current: number): number | undefined {
    const target = this.#forward.pop();
    if (target !== undefined) this.#back.push(current);
    this.#lastSeekAt = Number.NEGATIVE_INFINITY;
    return target;
  }

  get canGoBack(): boolean {
    return this.#back.length > 0;
  }

  get canGoForward(): boolean {
    return this.#forward.length > 0;
  }
}
