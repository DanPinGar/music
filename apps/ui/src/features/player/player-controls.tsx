import {
  ArrowRightToLineIcon,
  MinusIcon,
  MoveHorizontalIcon,
  PauseIcon,
  PinIcon,
  PlayIcon,
  PlusIcon,
  Undo2Icon,
  ZoomInIcon,
  ZoomOutIcon,
} from "lucide-react";
import { type MouseEvent, type RefObject, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "zustand";
import { useCommandRegistry } from "@/commands/commands-context";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { DEFAULT_MARKER_COLOR, type MarkerItem, sectionAt } from "./markers";
import { formatTimecode } from "./navigation";
import type { PlayerRuntime } from "./player-runtime";
import { GridControl, LoopControl } from "./practice-controls";
import type { TimelineController } from "./timeline/timeline-controller";
import type { FollowMode } from "./timeline/view";
import { usePlayback } from "./use-playback";

const FOLLOW_ICONS: Record<FollowMode, typeof PinIcon> = {
  page: ArrowRightToLineIcon,
  continuous: MoveHorizontalIcon,
  off: PinIcon,
};

/**
 * Transport bar under the timeline. Buttons give the focus back after a click, so Space and
 * the arrows keep driving the player instead of pressing the last clicked button.
 */
export function PlayerControls({
  videoRef,
  timeline,
  runtime,
  markers,
}: {
  videoRef: RefObject<HTMLVideoElement | null>;
  timeline: TimelineController;
  runtime: PlayerRuntime;
  markers: readonly MarkerItem[];
}) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const playback = usePlayback(videoRef);
  const { follow } = useSyncExternalStore(timeline.subscribe, timeline.getState);
  const stepUnit = useStore(runtime.session, (state) => state.stepUnit);
  const returnOnPause = useStore(runtime.session, (state) => state.returnOnPause);
  const FollowIcon = FOLLOW_ICONS[follow];
  const run = (command: string) => (event: MouseEvent<HTMLButtonElement>) => {
    event.currentTarget.blur();
    registry.execute(command);
  };
  const frame = Math.floor(playback.time * runtime.fps + 1e-6);
  const section = sectionAt(markers, playback.time);

  return (
    <div className="flex h-12 shrink-0 items-center gap-2 border-t px-4">
      <Button
        variant="ghost"
        size="icon"
        aria-label={playback.paused ? t("player.play") : t("player.pause")}
        onClick={run("player.togglePlay")}
      >
        {playback.paused ? <PlayIcon /> : <PauseIcon />}
      </Button>
      <span className="shrink-0 font-mono text-sm tabular-nums" data-testid="player-time">
        {formatTimecode(playback.time, playback.paused)}
        <span className="text-muted-foreground"> / {formatTimecode(playback.duration)}</span>
      </span>
      {playback.paused && (
        <span
          className="shrink-0 font-mono text-xs text-muted-foreground tabular-nums"
          title={t("player.frame")}
          data-testid="player-frame"
        >
          f{frame}
        </span>
      )}
      <div className="ml-2 flex min-w-0 items-center gap-1">
        <GridControl runtime={runtime} time={playback.time} />
        <Button
          variant="ghost"
          className="h-7 px-2 font-mono text-xs text-muted-foreground hover:text-foreground"
          title={t("commands.cycleStepUnit")}
          onClick={run("player.cycleStepUnit")}
          data-testid="step-unit"
        >
          {t("player.step", { unit: t(`player.stepUnits.${stepUnit}`) })}
        </Button>
        <LoopControl runtime={runtime} />
        {section && (
          <span
            className="flex min-w-0 items-center gap-1.5 truncate px-2 text-xs"
            data-testid="current-section"
          >
            <span
              className="size-2 shrink-0 rounded-full"
              style={{ background: section.color ?? DEFAULT_MARKER_COLOR }}
            />
            <span className="truncate">{section.label || `§${section.number}`}</span>
          </span>
        )}
      </div>
      <div className="flex-1" />
      <div className="flex shrink-0 items-center gap-1">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("commands.toggleReturnOnPause")}
          aria-pressed={returnOnPause}
          title={t("commands.toggleReturnOnPause")}
          className={cn(returnOnPause && "bg-muted text-foreground")}
          onClick={run("player.toggleReturnOnPause")}
          data-testid="return-on-pause"
        >
          <Undo2Icon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t(`timeline.follow.${follow}`)}
          title={t(`timeline.follow.${follow}`)}
          onClick={run("timeline.cycleFollow")}
          data-testid="timeline-follow"
        >
          <FollowIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("commands.zoomOut")}
          title={t("commands.zoomOut")}
          onClick={run("timeline.zoomOut")}
        >
          <ZoomOutIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("commands.zoomIn")}
          title={t("commands.zoomIn")}
          onClick={run("timeline.zoomIn")}
        >
          <ZoomInIcon />
        </Button>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("commands.slower")}
          onClick={run("player.slower")}
        >
          <MinusIcon />
        </Button>
        <span
          className="w-12 text-center font-mono text-sm tabular-nums"
          title={t("player.speed")}
          data-testid="player-rate"
        >
          {Math.round(playback.rate * 100)}%
        </span>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("commands.faster")}
          onClick={run("player.faster")}
        >
          <PlusIcon />
        </Button>
      </div>
    </div>
  );
}
