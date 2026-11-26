import { createStore, type StoreApi } from "zustand/vanilla";
import type { BeatGrid } from "./beat-grid";
import type { Loop, SpeedTrainer } from "./loop";

/** What the arrow keys step by. */
export type StepUnit = "seconds" | "beat" | "bar";
export const STEP_UNITS: readonly StepUnit[] = ["seconds", "beat", "bar"];

/** How loops are practised. */
export interface Practice {
  /** Beats played before the loop start on every repetition (0: none). */
  readonly preRollBeats: number;
  /** A bar of metronome clicks before playback starts. */
  readonly countIn: boolean;
  readonly trainer: SpeedTrainer | null;
}

/** The study state of the video open in the player, shared by commands, timeline and UI. */
export interface Session {
  readonly stepUnit: StepUnit;
  readonly grid: BeatGrid | null;
  readonly loop: Loop | null;
  readonly practice: Practice;
  /** Times the loop has wrapped round since it was set. */
  readonly repetitions: number;
  /** A count-in is playing; playback starts when it ends. */
  readonly countingIn: boolean;
  /** Pausing goes back to where playback started, as in a DAW. Kept for every video. */
  readonly returnOnPause: boolean;
  /** The saved state has been applied; from then on changes are saved. */
  readonly restored: boolean;
}

export type SessionStore = StoreApi<Session>;

const PREFERENCES_KEY = "segno.player";

export const NO_PRACTICE: Practice = { preRollBeats: 0, countIn: false, trainer: null };

export function createSession(): SessionStore {
  return createStore<Session>(() => ({
    stepUnit: "seconds",
    grid: null,
    loop: null,
    practice: NO_PRACTICE,
    repetitions: 0,
    countingIn: false,
    returnOnPause: loadReturnOnPause(),
    restored: false,
  }));
}

export function setReturnOnPause(session: SessionStore, value: boolean): void {
  session.setState({ returnOnPause: value });
  try {
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ returnOnPause: value }));
  } catch {
    // Storage unavailable: the preference lasts for this session only.
  }
}

function loadReturnOnPause(): boolean {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "null");
    return typeof stored === "object" && stored !== null && "returnOnPause" in stored
      ? stored.returnOnPause === true
      : false;
  } catch {
    return false;
  }
}
