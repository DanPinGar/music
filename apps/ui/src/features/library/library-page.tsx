import { expectData, type Video } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useApi } from "@/api/api-context";
import { Shortcut } from "@/commands/shortcut";
import { SegnoMark } from "@/components/segno-mark";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { formatDuration } from "@/lib/format";

/** Start screen: the videos downloaded or imported so far. */
export function LibraryPage() {
  const { t } = useTranslation();
  const api = useApi();
  const videos = useQuery({
    queryKey: ["videos"],
    queryFn: async () => expectData(await api.GET("/videos")),
  });

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-6 p-6">
      <div>
        <h1 className="font-heading text-2xl font-semibold">{t("library.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("library.subtitle")}</p>
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
          {t("library.emptyBody")} <Shortcut binding="Ctrl+K" className="align-middle" />
        </CardDescription>
      </CardHeader>
    </Card>
  );
}

function VideoGrid({ videos }: { videos: Video[] }) {
  const { t } = useTranslation();
  return (
    <ul className="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-4">
      {videos.map((video) => (
        <li key={video.id}>
          <Card>
            <CardHeader>
              <CardTitle className="line-clamp-2">{video.title}</CardTitle>
              <CardDescription className="flex items-center gap-2">
                <Badge variant={video.status === "failed" ? "destructive" : "secondary"}>
                  {t(`library.status.${video.status}`)}
                </Badge>
                {video.duration_ms != null && <span>{formatDuration(video.duration_ms)}</span>}
                {video.channel && <span className="truncate">{video.channel}</span>}
              </CardDescription>
            </CardHeader>
          </Card>
        </li>
      ))}
    </ul>
  );
}
