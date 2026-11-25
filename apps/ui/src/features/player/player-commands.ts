import type { Command } from "@/commands/registry";
import {
  clampTime,
  frameStep,
  LONG_STEP_SECONDS,
  nextRate,
  RATE_STEP,
  REPLAY_SECONDS,
  STEP_SECONDS,
} from "./navigation";

/** The player's commands, acting on whatever video element `video()` returns. */
export function playerCommands(video: () => HTMLVideoElement | null, fps: number): Command[] {
  const enabled = () => video() !== null;
  const withVideo = (action: (element: HTMLVideoElement) => void) => () => {
    const element = video();
    if (element) action(element);
  };
  const seekTo = (element: HTMLVideoElement, time: number) => {
    element.currentTime = clampTime(time, element.duration);
  };
  const play = (element: HTMLVideoElement) => {
    // play() rejects when a later pause() interrupts it; nothing to report then.
    element.play().catch(() => {});
  };
  const stepFrames = (frames: number) =>
    withVideo((element) => {
      element.pause();
      seekTo(element, frameStep(element.currentTime, fps, frames));
    });
  const seekBy = (seconds: number) =>
    withVideo((element) => seekTo(element, element.currentTime + seconds));
  const changeRate = (delta: number) =>
    withVideo((element) => {
      element.playbackRate = nextRate(element.playbackRate, delta);
    });

  return [
    {
      id: "player.togglePlay",
      title: "commands.togglePlay",
      group: "playback",
      keys: ["Space", "K"],
      enabled,
      run: withVideo((element) => (element.paused ? play(element) : element.pause())),
    },
    {
      id: "player.replay",
      title: "commands.replay",
      group: "playback",
      keys: ["Backspace"],
      enabled,
      run: withVideo((element) => {
        seekTo(element, element.currentTime - REPLAY_SECONDS);
        play(element);
      }),
    },
    {
      id: "player.slower",
      title: "commands.slower",
      group: "playback",
      keys: ["-"],
      enabled,
      run: changeRate(-RATE_STEP),
    },
    {
      id: "player.faster",
      title: "commands.faster",
      group: "playback",
      keys: ["+"],
      enabled,
      run: changeRate(RATE_STEP),
    },
    {
      id: "player.prevFrame",
      title: "commands.prevFrame",
      group: "navigation",
      keys: [","],
      enabled,
      run: stepFrames(-1),
    },
    {
      id: "player.nextFrame",
      title: "commands.nextFrame",
      group: "navigation",
      keys: ["."],
      enabled,
      run: stepFrames(1),
    },
    {
      id: "player.stepBack",
      title: "commands.stepBack",
      group: "navigation",
      keys: ["ArrowLeft"],
      enabled,
      run: seekBy(-STEP_SECONDS),
    },
    {
      id: "player.stepForward",
      title: "commands.stepForward",
      group: "navigation",
      keys: ["ArrowRight"],
      enabled,
      run: seekBy(STEP_SECONDS),
    },
    {
      id: "player.longBack",
      title: "commands.longBack",
      group: "navigation",
      keys: ["J"],
      enabled,
      run: seekBy(-LONG_STEP_SECONDS),
    },
    {
      id: "player.longForward",
      title: "commands.longForward",
      group: "navigation",
      keys: ["L"],
      enabled,
      run: seekBy(LONG_STEP_SECONDS),
    },
    {
      id: "player.goToStart",
      title: "commands.goToStart",
      group: "navigation",
      keys: ["Home"],
      enabled,
      run: withVideo((element) => seekTo(element, 0)),
    },
    {
      id: "player.goToEnd",
      title: "commands.goToEnd",
      group: "navigation",
      keys: ["End"],
      enabled,
      run: withVideo((element) => seekTo(element, element.duration)),
    },
  ];
}
