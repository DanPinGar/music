import { MinusIcon, PauseIcon, PlayIcon, PlusIcon } from "lucide-react";
import type { MouseEvent, RefObject } from "react";
import { useTranslation } from "react-i18next";
import { useCommandRegistry } from "@/commands/commands-context";
import { Button } from "@/components/ui/button";
import { formatTimecode } from "./navigation";
import { usePlayback } from "./use-playback";

/**
 * Transport bar under the video. Buttons give the focus back after a click, so Space and the
 * arrows keep driving the player instead of pressing the last clicked button.
 */
export function PlayerControls({
  videoRef,
  fps,
}: {
  videoRef: RefObject<HTMLVideoElement | null>;
  fps: number;
}) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const playback = usePlayback(videoRef);
  const run = (command: string) => (event: MouseEvent<HTMLButtonElement>) => {
    event.currentTarget.blur();
    registry.execute(command);
  };

  return (
    <div className="flex h-14 shrink-0 items-center gap-3 border-t px-4">
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
      <input
        type="range"
        className="min-w-0 flex-1 accent-primary"
        aria-label={t("player.position")}
        min={0}
        max={playback.duration || 0}
        step={1 / fps}
        value={playback.time}
        onChange={(event) => {
          const video = videoRef.current;
          if (video) video.currentTime = Number(event.currentTarget.value);
        }}
      />
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
