import { expectData, mediaUrl, type Video } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ListIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi, useApiConfig } from "@/api/api-context";
import { useSystemInfo } from "@/api/use-system-info";
import { useCommandRegistry, useCommands } from "@/commands/commands-context";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { PreparationStatus } from "@/features/library/preparation-status";
import { cn } from "@/lib/utils";
import { GoToDialog } from "./go-to-dialog";
import { forgetLastVideo, rememberLastVideo } from "./last-video";
import { MarkersPanel } from "./markers-panel";
import { type PlayerUi, playerCommands } from "./player-commands";
import { PlayerControls } from "./player-controls";
import { PlayerRuntime } from "./player-runtime";
import { createSession } from "./session";
import { type StudyUi, studyCommands } from "./study-commands";
import { Timeline } from "./timeline/timeline";
import { TimelineController } from "./timeline/timeline-controller";
import { type MarkerActions, useMarkers } from "./use-markers";
import { useAttacks, useWaveform } from "./use-navigation-data";
import { useSeamlessSource } from "./use-seamless-source";
import { useVideoState } from "./use-video-state";

const PANEL_KEY = "segno.player.markersPanel";

/** The study player for one video: loads it, then hands over to the player itself. */
export function VideoPlayerPage({ videoId }: { videoId: string }) {
  const { t } = useTranslation();
  const api = useApi();
  const video = useQuery({
    queryKey: ["videos", videoId],
    queryFn: async () =>
      expectData(await api.GET("/videos/{id}", { params: { path: { id: videoId } } })),
    // Follow the preparation; when the seek proxy is ready the player switches to it.
    refetchInterval: (query) => (query.state.data?.preparation ? 1_000 : false),
  });
  const library = useSystemInfo().data?.library_id;

  useEffect(() => {
    if (!library) return;
    if (video.isSuccess) rememberLastVideo(library, videoId);
    if (video.isError) forgetLastVideo(library);
  }, [library, video.isSuccess, video.isError, videoId]);

  if (video.isPending) {
    return <p className="p-6 text-sm text-muted-foreground">{t("common.loading")}</p>;
  }
  if (video.isError) {
    return (
      <div className="mx-auto max-w-xl p-6">
        <Card>
          <CardHeader>
            <CardTitle>{t("player.notFound")}</CardTitle>
            <CardDescription>{video.error.message}</CardDescription>
          </CardHeader>
          <CardContent>
            <Button variant="outline" render={<Link to="/" />}>
              {t("commands.goToLibrary")}
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }
  return <StudyPlayer video={video.data} />;
}

function StudyPlayer({ video }: { video: Video }) {
  const { t } = useTranslation();
  const config = useApiConfig();
  const registry = useCommandRegistry();
  const videoRef = useRef<HTMLVideoElement>(null);
  // The query parameter only makes the URL change when the API starts serving the proxy.
  const media = useSeamlessSource(
    videoRef,
    `${mediaUrl(config, video.id)}${video.seek_proxy ? "&copy=seek-proxy" : ""}`,
  );

  // One of each per video: the page is keyed by the video id.
  const runtime = useMemo(() => new PlayerRuntime(() => videoRef.current, createSession()), []);
  const timeline = useMemo(() => new TimelineController(() => videoRef.current), []);
  const { session } = runtime;
  const waveform = useWaveform(video);
  const attacks = useAttacks(video);
  const markers = useMarkers(video.id);
  const markersRef = useRef<MarkerActions>(markers);
  const [goToOpen, setGoToOpen] = useState(false);
  const [panelOpen, setPanelOpen] = useState(loadPanelOpen);

  const fps = video.fps ?? 30;
  const duration = (video.duration_ms ?? 0) / 1000;
  useEffect(() => runtime.setMedia(duration, fps), [runtime, duration, fps]);
  useEffect(() => runtime.setAttacks(attacks?.times ?? null), [runtime, attacks]);
  useEffect(() => {
    markersRef.current = markers;
    runtime.setMarkers(markers.items);
    timeline.setMarkers(markers.items);
  }, [runtime, timeline, markers]);
  useEffect(() => runtime.start(), [runtime]);
  useEffect(() => {
    const show = () => {
      const state = session.getState();
      timeline.setGrid(state.grid);
      timeline.setLoop(state.loop);
    };
    show();
    return session.subscribe(show);
  }, [session, timeline]);
  useVideoState(video.id, runtime, timeline);

  useEffect(() => {
    try {
      localStorage.setItem(PANEL_KEY, JSON.stringify(panelOpen));
    } catch {
      // Storage unavailable: the panel opens closed next time.
    }
  }, [panelOpen]);

  const ui = useMemo<PlayerUi & StudyUi>(
    () => ({
      openGoTo: () => setGoToOpen(true),
      toggleMarkersPanel: () => setPanelOpen((open) => !open),
      addMarker: (kind, start, end, color) =>
        markersRef.current.add({
          kind,
          start_ms: Math.round(start * 1000),
          end_ms: end === undefined ? null : Math.round(end * 1000),
          label: "",
          color: color ?? null,
        }),
      explain: (message) => toast(t(`loop.${message}`)),
    }),
    [t],
  );
  useEffect(() => {
    timeline.setHandlers({
      onLoopChange: (loop) => session.setState({ loop, repetitions: 0 }),
      onMarkerMove: (id, start, end) => {
        const item = markersRef.current.items.find((marker) => marker.id === id);
        if (!item) return;
        markersRef.current.change(id, {
          start_ms: Math.round(start * 1000),
          end_ms: end === null ? null : Math.round(end * 1000),
          label: item.label,
          color: item.color,
        });
      },
      onAddMarker: (time) => ui.addMarker("point", time),
    });
    return () => timeline.setHandlers(null);
  }, [timeline, session, ui]);

  const commands = useMemo(
    () => [...playerCommands(runtime, ui), ...studyCommands(runtime, ui)],
    [runtime, ui],
  );
  useCommands(commands);

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-3 border-b px-4 py-2">
        <h1 className="truncate font-heading text-base font-semibold">{video.title}</h1>
        {video.width && video.height && (
          <span className="shrink-0 text-xs text-muted-foreground">
            {video.width}×{video.height} · {Math.round(fps * 100) / 100} fps
          </span>
        )}
        <PreparationStatus video={video} className="ml-auto w-64 shrink-0" />
        <Button
          variant="ghost"
          size="sm"
          className={cn("shrink-0", !video.preparation && "ml-auto", panelOpen && "bg-muted")}
          aria-pressed={panelOpen}
          onClick={(event) => {
            event.currentTarget.blur();
            setPanelOpen((open) => !open);
          }}
          data-testid="markers-toggle"
        >
          <ListIcon />
          {t("markers.title")}
          <span className="text-muted-foreground">{markers.items.length}</span>
        </Button>
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="relative min-h-0 flex-1 bg-black">
            {/* biome-ignore lint/a11y/useMediaCaption: piano videos carry no speech to caption. */}
            <video
              ref={videoRef}
              src={media.src}
              onLoadedMetadata={media.onLoadedMetadata}
              preload="auto"
              playsInline
              data-testid="player-video"
              className="absolute inset-0 size-full object-contain"
              onClick={() => registry.execute("player.togglePlay")}
            />
          </div>
          <Timeline
            video={video}
            videoRef={videoRef}
            mediaSrc={media.src}
            timeline={timeline}
            waveform={waveform}
            attacks={attacks}
          />
          <PlayerControls
            videoRef={videoRef}
            timeline={timeline}
            runtime={runtime}
            markers={markers.items}
          />
        </div>
        {panelOpen && (
          <MarkersPanel runtime={runtime} markers={markers} onClose={() => setPanelOpen(false)} />
        )}
      </div>
      <GoToDialog
        open={goToOpen}
        onOpenChange={setGoToOpen}
        runtime={runtime}
        markers={markers.items}
      />
    </div>
  );
}

function loadPanelOpen(): boolean {
  try {
    return JSON.parse(localStorage.getItem(PANEL_KEY) ?? "false") === true;
  } catch {
    return false;
  }
}
