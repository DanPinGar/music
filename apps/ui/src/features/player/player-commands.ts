import type { Command } from "@/commands/registry";
import { nextAttack, previousAttack } from "./attacks";
import { nextBar, nextBeat, previousBar, previousBeat } from "./beat-grid";
import {
  frameStep,
  LONG_STEP_SECONDS,
  nextRate,
  RATE_STEP,
  REPLAY_SECONDS,
  STEP_SECONDS,
} from "./navigation";
import type { PlayerRuntime } from "./player-runtime";
import { STEP_UNITS, setReturnOnPause } from "./session";

/** What the player commands open in the UI. */
export interface PlayerUi {
  openGoTo(): void;
}

/** Playback and navigation: play, step by frame, attack, beat, bar or seconds, jump, history. */
export function playerCommands(runtime: PlayerRuntime, ui: PlayerUi): Command[] {
  const { session } = runtime;
  const enabled = () => runtime.video() !== null;
  const hasAttacks = () => enabled() && (runtime.attacks?.length ?? 0) > 0;
  const withVideo = (action: (element: HTMLVideoElement) => void) => () => {
    const element = runtime.video();
    if (element) action(element);
  };
  const stepFrames = (frames: number) =>
    withVideo((element) => {
      element.pause();
      runtime.seek(frameStep(element.currentTime, runtime.fps, frames));
    });
  const seekBy = (seconds: number) => () => runtime.seek(runtime.time() + seconds);
  /** ← and →: by the step unit, falling back to seconds without a beat grid. */
  const step = (direction: 1 | -1) => () => {
    const { stepUnit, grid } = session.getState();
    const time = runtime.time();
    const playing = runtime.playing();
    if (grid && stepUnit === "beat") {
      runtime.seek(direction > 0 ? nextBeat(grid, time) : previousBeat(grid, time, playing));
    } else if (grid && stepUnit === "bar") {
      runtime.seek(direction > 0 ? nextBar(grid, time) : previousBar(grid, time, playing));
    } else {
      runtime.seek(time + direction * STEP_SECONDS);
    }
  };
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
      run: () => runtime.togglePlay(),
    },
    {
      id: "player.replay",
      title: "commands.replay",
      group: "playback",
      keys: ["Backspace"],
      enabled,
      run: withVideo((element) => {
        runtime.seek(element.currentTime - REPLAY_SECONDS, { record: false });
        element.play().catch(() => {});
      }),
    },
    {
      id: "player.toggleReturnOnPause",
      title: "commands.toggleReturnOnPause",
      group: "playback",
      run: () => setReturnOnPause(session, !session.getState().returnOnPause),
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
      id: "player.prevAttack",
      title: "commands.prevAttack",
      group: "navigation",
      keys: ["Shift+ArrowLeft"],
      enabled: hasAttacks,
      run: () => {
        const time = previousAttack(runtime.attacks ?? [], runtime.time(), runtime.playing());
        if (time !== undefined) runtime.seek(time);
      },
    },
    {
      id: "player.nextAttack",
      title: "commands.nextAttack",
      group: "navigation",
      keys: ["Shift+ArrowRight"],
      enabled: hasAttacks,
      run: () => {
        const time = nextAttack(runtime.attacks ?? [], runtime.time());
        if (time !== undefined) runtime.seek(time);
      },
    },
    {
      id: "player.stepBack",
      title: "commands.stepBack",
      group: "navigation",
      keys: ["ArrowLeft"],
      enabled,
      run: step(-1),
    },
    {
      id: "player.stepForward",
      title: "commands.stepForward",
      group: "navigation",
      keys: ["ArrowRight"],
      enabled,
      run: step(1),
    },
    {
      id: "player.cycleStepUnit",
      title: "commands.cycleStepUnit",
      group: "navigation",
      keys: ["S"],
      enabled,
      run: () => {
        const { stepUnit, grid } = session.getState();
        const units = grid ? STEP_UNITS : (["seconds"] as const);
        const next = units[(units.indexOf(stepUnit) + 1) % units.length] ?? "seconds";
        session.setState({ stepUnit: next });
      },
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
      run: () => {
        const { loop } = session.getState();
        runtime.seek(loop?.enabled ? loop.start : 0);
      },
    },
    {
      id: "player.goToEnd",
      title: "commands.goToEnd",
      group: "navigation",
      keys: ["End"],
      enabled,
      run: () => {
        const { loop } = session.getState();
        // The loop's last frame: its very end would wrap straight round while playing.
        runtime.seek(loop?.enabled ? loop.end - 1 / runtime.fps : runtime.duration);
      },
    },
    {
      id: "player.goTo",
      title: "commands.goTo",
      group: "navigation",
      keys: ["G"],
      enabled,
      run: () => ui.openGoTo(),
    },
    {
      id: "player.historyBack",
      title: "commands.historyBack",
      group: "navigation",
      keys: ["Alt+ArrowLeft"],
      enabled: () => enabled() && runtime.history.canGoBack,
      run: () => {
        const time = runtime.history.back(runtime.time());
        if (time !== undefined) runtime.seek(time, { record: false });
      },
    },
    {
      id: "player.historyForward",
      title: "commands.historyForward",
      group: "navigation",
      keys: ["Alt+ArrowRight"],
      enabled: () => enabled() && runtime.history.canGoForward,
      run: () => {
        const time = runtime.history.forward(runtime.time());
        if (time !== undefined) runtime.seek(time, { record: false });
      },
    },
  ];
}
