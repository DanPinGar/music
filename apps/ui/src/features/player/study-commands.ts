import type { Marker } from "@segno/api-client";
import type { Command } from "@/commands/registry";
import { barTime, beatPosition, beatTime, withBeatsPerBar, withDownbeatAt } from "./beat-grid";
import { DEFAULT_TRAINER, type Loop, withEnd, withStart } from "./loop";
import {
  markerByNumber,
  nextMarkerTime,
  nextSectionColor,
  previousMarkerTime,
  sectionAt,
} from "./markers";
import type { PlayerRuntime } from "./player-runtime";

/** Without a beat grid, A or B alone makes a loop this long. */
const DEFAULT_LOOP_SECONDS = 4;
const BEATS_PER_BAR = [2, 3, 4, 5, 6, 7] as const;
const PRE_ROLLS = [0, 1, 2, 4] as const;

/** What the study commands need from the rest of the player. */
export interface StudyUi {
  addMarker(kind: Marker["kind"], start: number, end?: number, color?: string): void;
  toggleMarkersPanel(): void;
  /** Starts writing a comment at the playhead. */
  addComment(): void;
  showComments(): void;
  /** Tells the user why a command did nothing. */
  explain(message: "loopNeedsRange"): void;
}

/** Beat grid (tap tempo), loops and markers. */
export function studyCommands(runtime: PlayerRuntime, ui: StudyUi): Command[] {
  const { session } = runtime;
  const enabled = () => runtime.video() !== null;
  const hasGrid = () => session.getState().grid !== null;
  const hasLoop = () => session.getState().loop !== null;

  /** One bar at `time` with a grid, or a few seconds without. */
  const loopLength = (time: number) => {
    const { grid } = session.getState();
    if (!grid) return DEFAULT_LOOP_SECONDS;
    const beat = Math.floor(beatPosition(grid, time));
    return beatTime(grid, beat + grid.beatsPerBar) - beatTime(grid, beat);
  };
  /** A new or re-enabled loop starts counting again, at the trainer's first speed. */
  const setLoop = (loop: Loop | null) => {
    session.setState({ loop, repetitions: 0 });
    const trainer = session.getState().practice.trainer;
    const video = runtime.video();
    if (loop?.enabled && trainer && video) video.playbackRate = trainer.from;
  };

  return [
    {
      id: "grid.tap",
      title: "commands.tapTempo",
      group: "navigation",
      keys: ["T"],
      enabled,
      run: () => runtime.tap(),
    },
    {
      id: "grid.downbeatHere",
      title: "commands.downbeatHere",
      group: "navigation",
      enabled: hasGrid,
      run: () => {
        const { grid } = session.getState();
        if (grid) session.setState({ grid: withDownbeatAt(grid, runtime.time()) });
      },
    },
    {
      id: "grid.cycleBeatsPerBar",
      title: "commands.cycleBeatsPerBar",
      group: "navigation",
      enabled: hasGrid,
      run: () => {
        const { grid } = session.getState();
        if (!grid) return;
        const index = BEATS_PER_BAR.indexOf(grid.beatsPerBar as (typeof BEATS_PER_BAR)[number]);
        const next = BEATS_PER_BAR[(index + 1) % BEATS_PER_BAR.length] ?? 4;
        session.setState({ grid: withBeatsPerBar(grid, next) });
      },
    },
    {
      id: "grid.clear",
      title: "commands.clearGrid",
      group: "navigation",
      enabled: hasGrid,
      run: () => session.setState({ grid: null, stepUnit: "seconds" }),
    },
    {
      id: "loop.setStart",
      title: "commands.loopStart",
      group: "loops",
      keys: ["A"],
      enabled,
      run: () => {
        const time = runtime.time();
        setLoop(withStart(session.getState().loop, time, loopLength(time), runtime.duration));
      },
    },
    {
      id: "loop.setEnd",
      title: "commands.loopEnd",
      group: "loops",
      keys: ["B"],
      enabled,
      run: () => {
        const time = runtime.time();
        setLoop(withEnd(session.getState().loop, time, loopLength(Math.max(0, time - 1))));
      },
    },
    {
      id: "loop.toggle",
      title: "commands.toggleLoop",
      group: "loops",
      keys: ["R"],
      enabled,
      run: () => {
        const { loop, grid } = session.getState();
        if (loop) {
          setLoop({ ...loop, enabled: !loop.enabled });
          return;
        }
        const time = runtime.time();
        const section = sectionAt(runtime.markers, time);
        if (section?.end != null) {
          setLoop({ start: section.start, end: section.end, enabled: true });
        } else if (grid) {
          const beat = Math.floor(beatPosition(grid, time));
          const bar = Math.floor((beat - grid.downbeat) / grid.beatsPerBar) + 1;
          setLoop({ start: barTime(grid, bar), end: barTime(grid, bar + 1), enabled: true });
        } else {
          ui.explain("loopNeedsRange");
        }
      },
    },
    {
      id: "loop.clear",
      title: "commands.clearLoop",
      group: "loops",
      keys: ["Shift+R"],
      enabled: hasLoop,
      run: () => setLoop(null),
    },
    {
      id: "loop.save",
      title: "commands.saveLoop",
      group: "loops",
      enabled: hasLoop,
      run: () => {
        const { loop } = session.getState();
        if (loop) ui.addMarker("loop", loop.start, loop.end);
      },
    },
    {
      id: "loop.toSection",
      title: "commands.loopToSection",
      group: "markers",
      enabled: hasLoop,
      run: () => {
        const { loop } = session.getState();
        if (loop) {
          ui.addMarker("section", loop.start, loop.end, nextSectionColor(runtime.markers));
        }
      },
    },
    {
      id: "loop.cyclePreRoll",
      title: "commands.cyclePreRoll",
      group: "loops",
      run: () => {
        const { practice } = session.getState();
        const index = PRE_ROLLS.indexOf(practice.preRollBeats as (typeof PRE_ROLLS)[number]);
        const preRollBeats = PRE_ROLLS[(index + 1) % PRE_ROLLS.length] ?? 0;
        session.setState({ practice: { ...practice, preRollBeats } });
      },
    },
    {
      id: "loop.toggleCountIn",
      title: "commands.toggleCountIn",
      group: "loops",
      run: () => {
        const { practice } = session.getState();
        session.setState({ practice: { ...practice, countIn: !practice.countIn } });
      },
    },
    {
      id: "loop.toggleTrainer",
      title: "commands.toggleTrainer",
      group: "loops",
      run: () => {
        const { practice } = session.getState();
        const trainer = practice.trainer ? null : DEFAULT_TRAINER;
        session.setState({ practice: { ...practice, trainer }, repetitions: 0 });
        const video = runtime.video();
        if (video) video.playbackRate = trainer ? trainer.from : 1;
      },
    },
    {
      id: "markers.add",
      title: "commands.addMarker",
      group: "markers",
      keys: ["M"],
      enabled,
      run: () => ui.addMarker("point", runtime.time()),
    },
    {
      id: "markers.previous",
      title: "commands.previousMarker",
      group: "markers",
      keys: ["Ctrl+ArrowLeft"],
      enabled,
      run: () => {
        const time = previousMarkerTime(runtime.markers, runtime.time(), runtime.playing());
        if (time !== undefined) runtime.seek(time);
      },
    },
    {
      id: "markers.next",
      title: "commands.nextMarker",
      group: "markers",
      keys: ["Ctrl+ArrowRight"],
      enabled,
      run: () => {
        const time = nextMarkerTime(runtime.markers, runtime.time());
        if (time !== undefined) runtime.seek(time);
      },
    },
    {
      id: "markers.jump",
      title: "commands.jumpToMarker",
      group: "markers",
      keys: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],
      enabled,
      run: (trigger) => {
        const marker = markerByNumber(runtime.markers, Number(trigger?.binding) || 1);
        if (marker) runtime.seek(marker.start);
      },
    },
    {
      id: "markers.togglePanel",
      title: "commands.toggleMarkersPanel",
      group: "markers",
      run: () => ui.toggleMarkersPanel(),
    },
    {
      id: "comments.add",
      title: "commands.addComment",
      group: "comments",
      keys: ["C"],
      enabled,
      run: () => ui.addComment(),
    },
    {
      id: "comments.showPanel",
      title: "commands.showComments",
      group: "comments",
      run: () => ui.showComments(),
    },
  ];
}
