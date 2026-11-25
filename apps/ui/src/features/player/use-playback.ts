import { type RefObject, useEffect, useState } from "react";

export interface Playback {
  /** Current position in seconds. */
  readonly time: number;
  /** Length in seconds; 0 until the metadata has loaded. */
  readonly duration: number;
  readonly paused: boolean;
  readonly rate: number;
}

const INITIAL: Playback = { time: 0, duration: 0, paused: true, rate: 1 };

/**
 * Follows a video element: the position is refreshed every animation frame while playing
 * and on every jump while paused. Use it in small components, since they re-render often.
 */
export function usePlayback(ref: RefObject<HTMLVideoElement | null>): Playback {
  const [playback, setPlayback] = useState(INITIAL);

  useEffect(() => {
    const video = ref.current;
    if (!video) return;
    let frame = 0;
    const read = () =>
      setPlayback({
        time: video.currentTime,
        duration: Number.isFinite(video.duration) ? video.duration : 0,
        paused: video.paused,
        rate: video.playbackRate,
      });
    const tick = () => {
      read();
      frame = requestAnimationFrame(tick);
    };
    const sync = () => {
      cancelAnimationFrame(frame);
      read();
      if (!video.paused) frame = requestAnimationFrame(tick);
    };
    const events = [
      "play",
      "pause",
      "ended",
      "seeked",
      "seeking",
      "ratechange",
      "loadedmetadata",
      "durationchange",
    ];
    for (const event of events) video.addEventListener(event, sync);
    sync();
    return () => {
      cancelAnimationFrame(frame);
      for (const event of events) video.removeEventListener(event, sync);
    };
  }, [ref]);

  return playback;
}
