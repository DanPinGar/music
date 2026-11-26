import {
  ArrowRightToLineIcon,
  MinusIcon,
  MoveHorizontalIcon,
  PauseIcon,
  PinIcon,
  PlayIcon,
  PlusIcon,
  ZoomInIcon,
  ZoomOutIcon,
} from "lucide-react";
import { type MouseEvent, type RefObject, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { useCommandRegistry } from "@/commands/commands-context";
import { Button } from "@/components/ui/button";
import { formatTimecode } from "./navigation";
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
}: {
  videoRef: RefObject<HTMLVideoElement | null>;
  timeline: TimelineController;
}) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const playback = usePlayback(videoRef);
  const { follow } = useSyncExternalStore(timeline.subscribe, timeline.getState);
  const FollowIcon = FOLLOW_ICONS[follow];
  const run = (command: string) => (event: MouseEvent<HTMLButtonElement>) => {
    event.currentTarget.blur();
    registry.execute(command);
  };

  return (
    <div className="flex h-12 shrink-0 items-center gap-3 border-t px-4">
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
      <div className="flex-1" />
      <div className="flex shrink-0 items-center gap-1">
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
