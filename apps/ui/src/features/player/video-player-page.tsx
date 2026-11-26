import { expectData, mediaUrl } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useApi, useApiConfig } from "@/api/api-context";
import { useSystemInfo } from "@/api/use-system-info";
import { useCommandRegistry, useCommands } from "@/commands/commands-context";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { PreparationStatus } from "@/features/library/preparation-status";
import { forgetLastVideo, rememberLastVideo } from "./last-video";
import { playerCommands } from "./player-commands";
import { PlayerControls } from "./player-controls";
import { Timeline } from "./timeline/timeline";
import { TimelineController } from "./timeline/timeline-controller";
import { useAttacks, useWaveform } from "./use-navigation-data";
import { useSeamlessSource } from "./use-seamless-source";

/** The study player for one video. */
export function VideoPlayerPage({ videoId }: { videoId: string }) {
  const { t } = useTranslation();
  const api = useApi();
  const config = useApiConfig();
  const registry = useCommandRegistry();
  const videoRef = useRef<HTMLVideoElement>(null);
  const video = useQuery({
    queryKey: ["videos", videoId],
    queryFn: async () =>
      expectData(await api.GET("/videos/{id}", { params: { path: { id: videoId } } })),
    // Follow the preparation; when the seek proxy is ready the player switches to it.
    refetchInterval: (query) => (query.state.data?.preparation ? 1_000 : false),
  });
  // The query parameter only makes the URL change when the API starts serving the proxy.
  const proxyReady = video.data?.seek_proxy === true;
  const media = useSeamlessSource(
    videoRef,
    `${mediaUrl(config, videoId)}${proxyReady ? "&copy=seek-proxy" : ""}`,
  );

  const library = useSystemInfo().data?.library_id;

  // One timeline per video: the page is keyed by the video id.
  const timeline = useMemo(() => new TimelineController(() => videoRef.current), []);
  const waveform = useWaveform(video.data);
  const attacks = useAttacks(video.data);
  const attackTimes = useRef<readonly number[] | null>(null);
  useEffect(() => {
    attackTimes.current = attacks?.times ?? null;
  }, [attacks]);

  const fps = video.data?.fps ?? 30;
  const commands = useMemo(
    () =>
      playerCommands(
        () => videoRef.current,
        fps,
        () => attackTimes.current,
      ),
    [fps],
  );
  useCommands(commands);

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

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-3 border-b px-4 py-2">
        <h1 className="truncate font-heading text-base font-semibold">{video.data.title}</h1>
        {video.data.width && video.data.height && (
          <span className="shrink-0 text-xs text-muted-foreground">
            {video.data.width}×{video.data.height} · {Math.round(fps * 100) / 100} fps
          </span>
        )}
        <PreparationStatus video={video.data} className="ml-auto w-64 shrink-0" />
      </div>
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
        video={video.data}
        videoRef={videoRef}
        mediaSrc={media.src}
        timeline={timeline}
        waveform={waveform}
        attacks={attacks}
      />
      <PlayerControls videoRef={videoRef} timeline={timeline} />
    </div>
  );
}
