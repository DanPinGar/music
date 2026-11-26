import type { Video } from "@segno/api-client";
import { Link } from "@tanstack/react-router";
import { EllipsisIcon, PencilIcon, RotateCwIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import { DownloadStatus } from "./download-status";
import { studyProgress } from "./library-filters";
import { PreparationStatus } from "./preparation-status";
import { useLibraryActions } from "./use-library-actions";
import { DeleteVideoDialog, EditVideoDialog } from "./video-dialogs";
import { VideoThumbnail } from "./video-thumbnail";

/** "1920×1080 · 60 fps" for the videos that have media. */
function technicalLine(video: Video): string | undefined {
  const parts: string[] = [];
  if (video.width && video.height) parts.push(`${video.width}×${video.height}`);
  if (video.fps) parts.push(`${Math.round(video.fps * 100) / 100} fps`);
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

/**
 * A video in the library: opens in the player once ready, shows how far the user got, how
 * its download or preparation goes, and what can be done with it.
 */
export function VideoCard({
  video,
  activeTag,
  onTag,
}: {
  video: Video;
  activeTag: string | null;
  onTag: (tag: string | null) => void;
}) {
  const { t } = useTranslation();
  const { retry } = useLibraryActions();
  const ready = video.status === "ready";
  const progress = studyProgress(video);
  const failure = video.status === "failed";

  return (
    <Card
      className={cn(
        "relative h-full gap-0 overflow-hidden pt-0 transition-colors",
        ready &&
          "hover:bg-muted/40 has-[a:focus-visible]:ring-3 has-[a:focus-visible]:ring-ring/50",
      )}
      data-testid="video-card"
      data-video-id={video.id}
    >
      <div className="relative">
        <VideoThumbnail video={video} />
        {video.duration_ms != null && (
          <span className="absolute right-1.5 bottom-2 rounded bg-black/75 px-1 text-xs text-white tabular-nums">
            {formatDuration(video.duration_ms)}
          </span>
        )}
        {progress !== null && (
          <div className="absolute inset-x-0 bottom-0 h-1 bg-white/30" data-testid="study-progress">
            <div className="h-full bg-primary" style={{ width: `${progress * 100}%` }} />
          </div>
        )}
      </div>
      <CardHeader className="pt-4">
        <div className="flex items-start gap-1">
          <CardTitle className="line-clamp-2 flex-1">
            {ready ? (
              <Link
                to="/videos/$videoId"
                params={{ videoId: video.id }}
                // The whole card opens the video.
                className="outline-none after:absolute after:inset-0"
              >
                {video.title}
              </Link>
            ) : (
              video.title
            )}
          </CardTitle>
          <VideoMenu video={video} />
        </div>
        <CardDescription className="flex flex-wrap items-center gap-x-2 gap-y-1">
          {!ready && (
            <Badge variant={failure ? "destructive" : "secondary"}>
              {t(`library.status.${video.status}`)}
            </Badge>
          )}
          {video.channel && <span className="truncate">{video.channel}</span>}
          {technicalLine(video) && <span>{technicalLine(video)}</span>}
          {progress !== null && (
            <span>{t("library.resume", { time: formatDuration(video.position_ms ?? 0) })}</span>
          )}
        </CardDescription>
        {video.tags.length > 0 && (
          <div className="relative flex flex-wrap gap-1 pt-1">
            {video.tags.map((tag) => {
              const active = activeTag?.toLowerCase() === tag.toLowerCase();
              return (
                <button
                  key={tag}
                  type="button"
                  onClick={() => onTag(active ? null : tag)}
                  className={cn(
                    "rounded-full border px-2 py-0.5 text-xs transition-colors hover:bg-muted",
                    active && "border-primary bg-primary/10 text-foreground",
                  )}
                >
                  {tag}
                </button>
              );
            })}
          </div>
        )}
        <DownloadStatus video={video} className="pt-1" />
        {/* Above the card's link, for the failure's tooltip. */}
        <PreparationStatus video={video} className="relative pt-1" />
        {failure && !video.download && (
          <div className="relative flex items-center gap-2 pt-1">
            <span
              className="min-w-0 flex-1 truncate text-xs text-destructive"
              title={video.import_error ?? undefined}
            >
              {video.source === "youtube" ? t("library.downloadError") : t("library.importError")}
            </span>
            {video.source === "youtube" && (
              <Button size="xs" variant="outline" onClick={() => void retry(video)}>
                <RotateCwIcon />
                {t("common.retry")}
              </Button>
            )}
          </div>
        )}
      </CardHeader>
    </Card>
  );
}

/** "···": rename, tag or delete the video. */
function VideoMenu({ video }: { video: Video }) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-xs"
              className="relative -mt-0.5 -mr-1"
              aria-label={t("library.actions")}
              data-testid="video-menu"
            />
          }
        >
          <EllipsisIcon />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-auto">
          <DropdownMenuItem onClick={() => setEditing(true)} data-testid="video-edit">
            <PencilIcon />
            {t("library.edit")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            onClick={() => setDeleting(true)}
            data-testid="video-delete"
          >
            <Trash2Icon />
            {t("library.delete")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <EditVideoDialog video={video} open={editing} onOpenChange={setEditing} />
      <DeleteVideoDialog video={video} open={deleting} onOpenChange={setDeleting} />
    </>
  );
}
