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
import { forgetLastVideo, rememberLastVideo } from "./last-video";
import { playerCommands } from "./player-commands";
import { PlayerControls } from "./player-controls";

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
  });

  const library = useSystemInfo().data?.library_id;

  const fps = video.data?.fps ?? 30;
  const commands = useMemo(() => playerCommands(() => videoRef.current, fps), [fps]);
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
      <div className="flex shrink-0 items-baseline gap-3 border-b px-4 py-2">
        <h1 className="truncate font-heading text-base font-semibold">{video.data.title}</h1>
        {video.data.width && video.data.height && (
          <span className="shrink-0 text-xs text-muted-foreground">
            {video.data.width}×{video.data.height} · {Math.round(fps * 100) / 100} fps
          </span>
        )}
      </div>
      <div className="relative min-h-0 flex-1 bg-black">
        {/* biome-ignore lint/a11y/useMediaCaption: piano videos carry no speech to caption. */}
        <video
          ref={videoRef}
          src={mediaUrl(config, videoId)}
          preload="auto"
          playsInline
          className="absolute inset-0 size-full object-contain"
          onClick={() => registry.execute("player.togglePlay")}
        />
      </div>
      <PlayerControls videoRef={videoRef} fps={fps} />
    </div>
  );
}
