import { useEffect } from "react";
import { useApi } from "@/api/api-context";
import type { PlayerRuntime } from "./player-runtime";
import type { TimelineController } from "./timeline/timeline-controller";
import { fromApi, type StudyState, toApi } from "./video-state";

/** Saves at most this often (ms) while things change, and while playing. */
const SAVE_EVERY = 1000;
const SAVE_WHILE_PLAYING = 5000;

/**
 * Reopens the video where it was left (position, speed, zoom, beat grid, loop and practice
 * settings) and saves changes from then on, including when the player closes.
 */
export function useVideoState(
  videoId: string,
  runtime: PlayerRuntime,
  timeline: TimelineController,
) {
  const api = useApi();

  useEffect(() => {
    const video = runtime.video();
    if (!video) return;
    const { session } = runtime;
    const path = { params: { path: { id: videoId } } };
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const current = (): StudyState => {
      const state = session.getState();
      return {
        position: video.currentTime,
        speed: video.playbackRate,
        stepUnit: state.stepUnit,
        view: timeline.view,
        grid: state.grid,
        loop: state.loop,
        practice: state.practice,
      };
    };
    const save = () => {
      if (timer) clearTimeout(timer);
      timer = null;
      if (!session.getState().restored) return;
      void api.PUT("/videos/{id}/state", { ...path, body: toApi(current()) });
    };
    const schedule = () => {
      timer ??= setTimeout(save, SAVE_EVERY);
    };

    const restore = (state: StudyState) => {
      session.setState({
        stepUnit: state.stepUnit,
        grid: state.grid,
        loop: state.loop,
        practice: state.practice,
        restored: true,
      });
      if (state.view) timeline.setView(state.view);
      const place = () => {
        runtime.seek(state.position, { record: false });
        video.playbackRate = state.speed;
      };
      if (video.readyState >= 1) place();
      else video.addEventListener("loadedmetadata", place, { once: true });
    };
    void api.GET("/videos/{id}/state", path).then(
      (result) => {
        if (cancelled) return;
        if (result.data) restore(fromApi(result.data));
        else session.setState({ restored: true });
      },
      () => session.setState({ restored: true }),
    );

    const unsubscribe = session.subscribe((state, previous) => {
      if (
        state.grid !== previous.grid ||
        state.loop !== previous.loop ||
        state.practice !== previous.practice ||
        state.stepUnit !== previous.stepUnit
      ) {
        schedule();
      }
    });
    const unsubscribeView = timeline.subscribeView(schedule);
    const events = ["pause", "seeked", "ratechange"] as const;
    for (const event of events) video.addEventListener(event, schedule);
    const whilePlaying = setInterval(() => {
      if (!video.paused) schedule();
    }, SAVE_WHILE_PLAYING);

    return () => {
      cancelled = true;
      clearInterval(whilePlaying);
      for (const event of events) video.removeEventListener(event, schedule);
      unsubscribe();
      unsubscribeView();
      save();
    };
  }, [api, videoId, runtime, timeline]);
}
