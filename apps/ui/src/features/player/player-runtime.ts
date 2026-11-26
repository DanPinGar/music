import { type BeatGrid, beatPosition, beatTime, mergeTaps, snapTaps } from "./beat-grid";
import { NavigationHistory } from "./history";
import { type Loop, mustWrap, trainerSpeed } from "./loop";
import type { MarkerItem } from "./markers";
import { type CountIn, countIn } from "./metronome";
import { clampTime } from "./navigation";
import type { SessionStore } from "./session";

/** Without a beat grid, a pre-roll or count-in beat lasts this long (120 bpm). */
const DEFAULT_BEAT = 0.5;
/** A tap more than this after the previous one (in the music) starts a new run of taps. */
const TAP_RUN_GAP = 2;
/** The loop wrap is timed with a timer once its end is this close (seconds of music). */
const WRAP_LOOKAHEAD = 0.06;
const LOOP_GAPS_KEPT = 20;

export interface SeekOptions {
  /** Keep the seek out of the navigation history (loop wraps, returning to a start…). */
  readonly record?: boolean;
}

/**
 * The player's moving parts outside React: seeks (and the history they feed), playback with
 * count-in, pre-roll and return-on-pause, the loop and tap tempo. Commands, the timeline and
 * the UI all go through it.
 */
export class PlayerRuntime {
  readonly session: SessionStore;
  readonly history = new NavigationHistory();
  readonly #video: () => HTMLVideoElement | null;
  #fps = 30;
  #duration = 0;
  #attacks: readonly number[] | null = null;
  #markers: readonly MarkerItem[] = [];
  /** The position before the current seek: where a jump starts from. */
  #stable = 0;
  #suppress = false;
  /** Where the last playback started (for return-on-pause). */
  #playedFrom: number | null = null;
  #countIn: CountIn | null = null;
  #wrapTimer: ReturnType<typeof setTimeout> | null = null;
  #wrappedAt: number | null = null;
  #tapBase: BeatGrid | null = null;
  #taps: number[] = [];
  /** Milliseconds from each loop wrap to the first frame shown after it. */
  readonly loopGaps: number[] = [];

  constructor(video: () => HTMLVideoElement | null, session: SessionStore) {
    this.#video = video;
    this.session = session;
  }

  video(): HTMLVideoElement | null {
    return this.#video();
  }

  get fps(): number {
    return this.#fps;
  }

  get duration(): number {
    const video = this.#video();
    return video && Number.isFinite(video.duration) && video.duration > 0
      ? video.duration
      : this.#duration;
  }

  get attacks(): readonly number[] | null {
    return this.#attacks;
  }

  get markers(): readonly MarkerItem[] {
    return this.#markers;
  }

  setMedia(duration: number, fps: number): void {
    this.#duration = duration;
    this.#fps = fps > 0 ? fps : 30;
  }

  setAttacks(times: readonly number[] | null): void {
    this.#attacks = times;
  }

  setMarkers(items: readonly MarkerItem[]): void {
    this.#markers = items;
  }

  /** Current position, in seconds. */
  time(): number {
    return this.#video()?.currentTime ?? 0;
  }

  playing(): boolean {
    const video = this.#video();
    return video !== null && !video.paused;
  }

  seek(time: number, options: SeekOptions = {}): void {
    const video = this.#video();
    if (!video) return;
    if (options.record === false) this.#suppress = true;
    video.currentTime = clampTime(time, this.duration);
  }

  /** Space: play (after the pre-roll and count-in) or pause (and return, if asked to). */
  togglePlay(): void {
    const video = this.#video();
    if (!video) return;
    if (this.#countIn) {
      this.#countIn.cancel();
      return;
    }
    if (!video.paused) {
      video.pause();
      const from = this.#playedFrom;
      if (this.session.getState().returnOnPause && from !== null)
        this.seek(from, { record: false });
      return;
    }
    const { loop, practice, grid } = this.session.getState();
    this.#playedFrom = video.currentTime;
    if (
      loop?.enabled &&
      practice.preRollBeats > 0 &&
      Math.abs(video.currentTime - loop.start) < 0.05
    ) {
      this.seek(loop.start - this.preRollSeconds(loop), { record: false });
    }
    if (!practice.countIn) {
      this.#play(video);
      return;
    }
    const beats = grid?.beatsPerBar ?? 4;
    const count = countIn(beats, this.#beatLength(grid, video.currentTime));
    this.#countIn = count;
    this.session.setState({ countingIn: true });
    void count.done.then((completed) => {
      this.#countIn = null;
      this.session.setState({ countingIn: false });
      if (completed) this.#play(video);
    });
  }

  /** Seconds of pre-roll before `loop`: whole beats of the grid, or of 120 bpm without one. */
  preRollSeconds(loop: Loop): number {
    const { practice, grid } = this.session.getState();
    if (practice.preRollBeats <= 0) return 0;
    if (!grid) return practice.preRollBeats * DEFAULT_BEAT;
    const startBeat = Math.round(beatPosition(grid, loop.start));
    return Math.max(0, loop.start - beatTime(grid, startBeat - practice.preRollBeats));
  }

  /** T: one tap on the beat. A run of taps replaces the grid's beats where it was tapped. */
  tap(): void {
    const video = this.#video();
    if (!video) return;
    const time = video.currentTime;
    const last = this.#taps.at(-1);
    if (last === undefined || time <= last || time - last > TAP_RUN_GAP) {
      this.#tapBase = this.session.getState().grid;
      this.#taps = [];
    }
    this.#taps.push(time);
    const grid = mergeTaps(this.#tapBase, snapTaps(this.#taps, this.#attacks));
    if (grid) this.session.setState({ grid });
  }

  /** Starts following the video; returns a function that stops. */
  start(): () => void {
    const video = this.#video();
    if (!video) return () => {};
    const onSeeking = () => {
      if (this.#suppress) {
        this.#suppress = false;
        return;
      }
      this.history.noteSeek(this.#stable, video.currentTime, performance.now());
    };
    video.addEventListener("seeking", onSeeking);
    let frame = 0;
    const loop = () => {
      this.#tick(video);
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(frame);
      video.removeEventListener("seeking", onSeeking);
      this.#countIn?.cancel();
      if (this.#wrapTimer) clearTimeout(this.#wrapTimer);
    };
  }

  #play(video: HTMLVideoElement): void {
    // play() rejects when a later pause() interrupts it; nothing to report then.
    video.play().catch(() => {});
  }

  #beatLength(grid: BeatGrid | null, time: number): number {
    if (!grid) return DEFAULT_BEAT;
    const index = Math.floor(beatPosition(grid, time));
    return beatTime(grid, index + 1) - beatTime(grid, index);
  }

  #tick(video: HTMLVideoElement): void {
    if (video.seeking) return;
    const time = video.currentTime;
    const { loop } = this.session.getState();
    if (loop?.enabled && !video.paused) {
      const preRoll = this.preRollSeconds(loop);
      if (mustWrap(loop, this.#stable, time, preRoll)) {
        this.#wrap(video, loop, preRoll);
        return;
      }
      // A frame is too coarse to wrap on time: finish the last stretch with a timer.
      const remaining = loop.end - time;
      const inside = time >= loop.start - preRoll - 0.05;
      if (inside && remaining > 0 && remaining <= WRAP_LOOKAHEAD && !this.#wrapTimer) {
        const delay = (remaining / video.playbackRate) * 1000 - 2;
        this.#wrapTimer = setTimeout(
          () => {
            this.#wrapTimer = null;
            const current = this.session.getState().loop;
            if (current?.enabled && !video.paused && video.currentTime >= current.end - 0.008) {
              this.#wrap(video, current, this.preRollSeconds(current));
            }
          },
          Math.max(0, delay),
        );
      }
    }
    this.#stable = time;
  }

  #wrap(video: HTMLVideoElement, loop: Loop, preRoll: number): void {
    if (this.#wrapTimer) {
      clearTimeout(this.#wrapTimer);
      this.#wrapTimer = null;
    }
    const repetitions = this.session.getState().repetitions + 1;
    const { trainer } = this.session.getState().practice;
    this.session.setState({ repetitions });
    if (trainer) video.playbackRate = trainerSpeed(trainer, repetitions);
    this.#wrappedAt = performance.now();
    const target = Math.max(0, loop.start - preRoll);
    this.#stable = target;
    this.seek(target, { record: false });
    video.requestVideoFrameCallback(() => {
      if (this.#wrappedAt === null) return;
      this.loopGaps.push(Math.round(performance.now() - this.#wrappedAt));
      if (this.loopGaps.length > LOOP_GAPS_KEPT) this.loopGaps.shift();
      video.dataset.loopGaps = this.loopGaps.join(",");
      this.#wrappedAt = null;
    });
  }
}
