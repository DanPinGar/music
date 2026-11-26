import type { Command } from "@/commands/registry";
import type { TimelineController } from "./timeline-controller";

/** Zoom and follow commands of the timeline. */
export function timelineCommands(timeline: TimelineController): Command[] {
  return [
    {
      id: "timeline.zoomIn",
      title: "commands.zoomIn",
      group: "view",
      keys: ["Ctrl++", "Ctrl+="],
      run: timeline.zoomIn,
    },
    {
      id: "timeline.zoomOut",
      title: "commands.zoomOut",
      group: "view",
      keys: ["Ctrl+-"],
      run: timeline.zoomOut,
    },
    {
      id: "timeline.zoomFit",
      title: "commands.zoomFit",
      group: "view",
      keys: ["Ctrl+0"],
      run: timeline.zoomFit,
    },
    {
      id: "timeline.cycleFollow",
      title: "commands.cycleFollow",
      group: "view",
      run: timeline.cycleFollow,
    },
  ];
}
