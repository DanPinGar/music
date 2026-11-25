import { type RefObject, useCallback, useEffect, useRef, useState } from "react";

interface Resume {
  readonly time: number;
  readonly rate: number;
  readonly playing: boolean;
}

/**
 * The `<video>` source, switched without losing your place: when `wanted` changes (e.g. the
 * seek proxy becomes ready), the position, speed and play state carry over to the new source.
 * Pass the returned `onLoadedMetadata` to the element.
 */
export function useSeamlessSource(ref: RefObject<HTMLVideoElement | null>, wanted: string) {
  const [active, setActive] = useState(wanted);
  const resume = useRef<Resume | null>(null);

  useEffect(() => {
    if (wanted === active) return;
    // Read the old source's state before React swaps `src` (loading resets it).
    const video = ref.current;
    resume.current = video
      ? { time: video.currentTime, rate: video.playbackRate, playing: !video.paused }
      : null;
    setActive(wanted);
  }, [wanted, active, ref]);

  const onLoadedMetadata = useCallback(() => {
    const video = ref.current;
    const state = resume.current;
    if (!video || !state) return;
    resume.current = null;
    video.currentTime = state.time;
    video.playbackRate = state.rate;
    if (state.playing) video.play().catch(() => {});
  }, [ref]);

  return { src: active, onLoadedMetadata };
}
