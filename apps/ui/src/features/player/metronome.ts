/** Seconds the video takes to start sounding after `play()` (measured: 60–75 ms). */
export const PLAY_LATENCY = 0.065;

let context: AudioContext | null = null;

function audio(): AudioContext {
  context ??= new AudioContext();
  if (context.state === "suspended") void context.resume();
  return context;
}

/** A playing count-in; `done` resolves to false if it was cancelled. */
export interface CountIn {
  readonly done: Promise<boolean>;
  cancel(): void;
}

/**
 * Plays `beats` clicks `period` seconds apart (the first one accented) and resolves when the
 * next beat is due, minus the video's start latency, so playback lands on that beat.
 */
export function countIn(beats: number, period: number): CountIn {
  const ctx = audio();
  const start = ctx.currentTime + 0.05;
  const sources: OscillatorNode[] = [];
  for (let beat = 0; beat < beats; beat++) {
    const at = start + beat * period;
    const oscillator = ctx.createOscillator();
    const gain = ctx.createGain();
    oscillator.frequency.value = beat === 0 ? 1600 : 1000;
    gain.gain.setValueAtTime(0.0001, at);
    gain.gain.exponentialRampToValueAtTime(0.5, at + 0.002);
    gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.06);
    oscillator.connect(gain).connect(ctx.destination);
    oscillator.start(at);
    oscillator.stop(at + 0.07);
    sources.push(oscillator);
  }
  let finish: (completed: boolean) => void = () => {};
  const done = new Promise<boolean>((resolve) => {
    finish = resolve;
  });
  const wait = Math.max(0, (start + beats * period - PLAY_LATENCY - ctx.currentTime) * 1000);
  const timer = setTimeout(() => finish(true), wait);
  return {
    done,
    cancel: () => {
      clearTimeout(timer);
      for (const source of sources) {
        try {
          source.stop();
        } catch {
          // Already stopped.
        }
      }
      finish(false);
    },
  };
}
