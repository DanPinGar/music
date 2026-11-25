import { expectData, type Video } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { FilePlusIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/api/api-context";
import { Shortcut } from "@/commands/shortcut";
import { SegnoMark } from "@/components/segno-mark";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { formatDuration } from "@/lib/format";
import { useImportVideos } from "./use-import-videos";

/** Start screen: the videos downloaded or imported so far. */
export function LibraryPage() {
  const { t } = useTranslation();
  const api = useApi();
  const { pickAndImport } = useImportVideos();
  const videos = useQuery({
    queryKey: ["videos"],
    queryFn: async () => expectData(await api.GET("/videos")),
  });

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-6 p-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="font-heading text-2xl font-semibold">{t("library.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("library.subtitle")}</p>
        </div>
        <Button onClick={() => void pickAndImport()}>
          <FilePlusIcon />
          {t("library.import")}
          <Shortcut binding="Ctrl+O" className="ml-1" />
        </Button>
      </div>

      {videos.isPending && <p className="text-sm text-muted-foreground">{t("common.loading")}</p>}

      {videos.isError && (
        <Card>
          <CardHeader>
            <CardTitle>{t("library.error")}</CardTitle>
            <CardDescription>{videos.error.message}</CardDescription>
          </CardHeader>
          <CardContent>
            <Button variant="outline" onClick={() => void videos.refetch()}>
              {t("common.retry")}
            </Button>
          </CardContent>
        </Card>
      )}

      {videos.isSuccess && videos.data.length === 0 && <EmptyLibrary />}
      {videos.isSuccess && videos.data.length > 0 && <VideoGrid videos={videos.data} />}
    </div>
  );
}

function EmptyLibrary() {
  const { t } = useTranslation();
  return (
    <Card className="items-center py-12 text-center">
      <SegnoMark className="size-12 text-muted-foreground" />
      <CardHeader className="w-full max-w-md">
        <CardTitle>{t("library.emptyTitle")}</CardTitle>
        <CardDescription>
          {t("library.emptyBody")} <Shortcut binding="Ctrl+O" className="align-middle" />
        </CardDescription>
      </CardHeader>
    </Card>
  );
}

/** "1920×1080 · 60 fps" for the videos that have media. */
function technicalLine(video: Video): string | undefined {
  const parts: string[] = [];
  if (video.width && video.height) parts.push(`${video.width}×${video.height}`);
  if (video.fps) parts.push(`${Math.round(video.fps * 100) / 100} fps`);
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

function VideoGrid({ videos }: { videos: Video[] }) {
  const { t } = useTranslation();
  return (
    <ul className="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-4">
      {videos.map((video) => (
        <li key={video.id}>
          <Link
            to="/videos/$videoId"
            params={{ videoId: video.id }}
            className="block rounded-xl outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          >
            <Card className="h-full transition-colors hover:bg-muted/40">
              <CardHeader>
                <CardTitle className="line-clamp-2">{video.title}</CardTitle>
                <CardDescription className="flex flex-wrap items-center gap-2">
                  <Badge variant={video.status === "failed" ? "destructive" : "secondary"}>
                    {t(`library.status.${video.status}`)}
                  </Badge>
                  {video.duration_ms != null && <span>{formatDuration(video.duration_ms)}</span>}
                  {technicalLine(video) && <span>{technicalLine(video)}</span>}
                  {video.channel && <span className="truncate">{video.channel}</span>}
                </CardDescription>
              </CardHeader>
            </Card>
          </Link>
        </li>
      ))}
    </ul>
  );
}
