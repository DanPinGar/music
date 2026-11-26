import { type Attacks, assetUrl, type Video } from "@segno/api-client";
import { type RefObject, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { useApiConfig } from "@/api/api-context";
import { useCommands } from "@/commands/commands-context";
import { tilePosition } from "@/features/library/video-thumbnail";
import { cn } from "@/lib/utils";
import { type CommentItem, commentsAt, excerpt } from "../comments";
import { formatTimecode } from "../navigation";
import { timelineCommands } from "./timeline-commands";
import type { Hover, TimelineController } from "./timeline-controller";
import type { Waveform } from "./waveform";

const PREVIEW_WIDTH = 192;

/**
 * The two-level timeline under the video: the whole song (overview) and a zoomable detail
 * view with the waveform, the ruler and the attacks. Hovering shows the frame at that time.
 */
export function Timeline({
  video,
  videoRef,
  mediaSrc,
  timeline,
  waveform,
  attacks,
  comments,
}: {
  video: Video;
  videoRef: RefObject<HTMLVideoElement | null>;
  /** The player's source, for the exact-frame preview. */
  mediaSrc: string;
  timeline: TimelineController;
  waveform: Waveform | null;
  attacks: Attacks | null;
  /** Shown in the hover preview at their moment. */
  comments: readonly CommentItem[];
}) {
  const { t } = useTranslation();
  const overviewRef = useRef<HTMLCanvasElement>(null);
  const detailRef = useRef<HTMLCanvasElement>(null);
  const { hover } = useSyncExternalStore(timeline.subscribe, timeline.getState);
  useCommands(useMemo(() => timelineCommands(timeline), [timeline]));

  const duration = (video.duration_ms ?? 0) / 1000;
  const fps = video.fps ?? 30;
  useEffect(() => timeline.setMedia(duration, fps), [timeline, duration, fps]);
  useEffect(() => timeline.setWaveform(waveform), [timeline, waveform]);
  useEffect(() => timeline.setAttacks(attacks), [timeline, attacks]);

  useEffect(() => {
    const overview = overviewRef.current;
    const detail = detailRef.current;
    const element = videoRef.current;
    if (!overview || !detail || !element) return;
    const detach = timeline.attach(overview, detail);
    const onSeeking = () => timeline.noteJump();
    const onPlay = () => timeline.notePlay();
    element.addEventListener("seeking", onSeeking);
    element.addEventListener("play", onPlay);
    let frame = 0;
    const loop = () => {
      timeline.frame();
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(frame);
      element.removeEventListener("seeking", onSeeking);
      element.removeEventListener("play", onPlay);
      detach();
    };
  }, [timeline, videoRef]);

  return (
    <div className="relative shrink-0 select-none border-t">
      <canvas
        ref={overviewRef}
        role="slider"
        tabIndex={0}
        aria-label={t("timeline.overview")}
        aria-valuemin={0}
        aria-valuemax={Math.round(duration)}
        aria-valuenow={0}
        className="block h-7 w-full cursor-pointer touch-none outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        data-testid="timeline-overview"
      />
      <canvas
        ref={detailRef}
        aria-label={t("timeline.detail")}
        role="img"
        className="block h-24 w-full cursor-pointer touch-none border-t"
        data-testid="timeline-detail"
      />
      <FramePreview video={video} mediaSrc={mediaSrc} hover={hover} comments={comments} />
    </div>
  );
}

/**
 * The frame under the pointer, above the timeline. Over the overview it shows the thumbnail
 * of that second; over the detail view, the exact frame from a second, hidden video that
 * only ever runs the latest seek, with the thumbnail until that frame is ready.
 */
function FramePreview({
  video,
  mediaSrc,
  hover,
  comments,
}: {
  video: Video;
  mediaSrc: string;
  hover: Hover | null;
  comments: readonly CommentItem[];
}) {
  const said = hover ? commentsAt(comments, hover.time)[0] : undefined;
  const config = useApiConfig();
  const frameRef = useRef<HTMLVideoElement>(null);
  const seeking = useRef<{ busy: boolean; current: number; next: number | null }>({
    busy: false,
    current: 0,
    next: null,
  });
  const [shown, setShown] = useState<number | null>(null);
  const exact = hover?.surface === "detail";
  const wanted = exact ? hover.time : null;

  useEffect(() => {
    const element = frameRef.current;
    if (!element || wanted === null) return;
    const state = seeking.current;
    if (state.busy) {
      state.next = wanted;
      return;
    }
    state.busy = true;
    state.current = wanted;
    element.currentTime = wanted;
  }, [wanted]);

  const onSeeked = () => {
    const element = frameRef.current;
    const state = seeking.current;
    setShown(state.current);
    if (element && state.next !== null) {
      state.current = state.next;
      state.next = null;
      element.currentTime = state.current;
    } else {
      state.busy = false;
    }
  };
  const onEmptied = () => {
    seeking.current = { busy: false, current: 0, next: null };
    setShown(null);
  };

  const sprite = video.thumbnails;
  const height = sprite
    ? Math.round((PREVIEW_WIDTH * sprite.tile_height) / sprite.tile_width)
    : Math.round((PREVIEW_WIDTH * 9) / 16);
  const left = hover
    ? Math.min(Math.max(hover.x - PREVIEW_WIDTH / 2, 4), hover.width - PREVIEW_WIDTH - 4)
    : 0;
  const tile =
    sprite && hover
      ? Math.min(sprite.count - 1, Math.floor((hover.time * 1000) / sprite.interval_ms))
      : 0;
  const showFrame = exact && shown !== null && Math.abs(shown - hover.time) < 0.25;

  return (
    <div
      className={cn(
        "pointer-events-none absolute bottom-full z-10 mb-2 overflow-hidden rounded-md border bg-black shadow-lg",
        !hover && "hidden",
      )}
      style={{ left, width: PREVIEW_WIDTH }}
      data-testid="timeline-preview"
      data-exact={showFrame ? "true" : "false"}
    >
      <div className="relative" style={{ height }}>
        {sprite && (
          <div
            className="absolute inset-0 bg-no-repeat"
            style={{
              backgroundImage: `url("${assetUrl(config, video.id, "thumbnails")}")`,
              backgroundSize: `${sprite.columns * 100}% ${sprite.rows * 100}%`,
              backgroundPosition: tilePosition(tile, sprite.columns, sprite.rows),
            }}
          />
        )}
        <video
          ref={frameRef}
          src={mediaSrc}
          muted
          preload="metadata"
          playsInline
          onSeeked={onSeeked}
          onEmptied={onEmptied}
          className={cn("absolute inset-0 size-full object-contain", !showFrame && "invisible")}
        />
      </div>
      <div className="bg-black/80 px-2 py-0.5 text-center font-mono text-xs text-white/90 tabular-nums">
        {hover ? formatTimecode(hover.time, true) : ""}
      </div>
      {said && (
        <div
          className="border-t border-violet-400/30 bg-black/85 px-2 py-1 text-xs text-violet-200"
          data-testid="timeline-preview-comment"
        >
          {excerpt(said.body)}
        </div>
      )}
    </div>
  );
}
