import type { Video } from "@segno/api-client";
import { Link } from "@tanstack/react-router";
import { EllipsisIcon, PencilIcon, RotateCwIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
import { songDetails, videoName } from "./library-filters";
import { PreparationStatus } from "./preparation-status";
import { useLibraryActions } from "./use-library-actions";
import { DeleteVideoDialog, EditVideoDialog } from "./video-dialogs";
import { VideoThumbnail } from "./video-thumbnail";

/**
 * A video in the library's list: its name on one line, under it its duration and how its
 * download or preparation goes, and a small picture at the end. Opens in the player once
 * ready.
 */
export function VideoRow({ video }: { video: Video }) {
  const { t } = useTranslation();
  const { retry } = useLibraryActions();
  const ready = video.status === "ready";
  const failure = video.status === "failed";
  const name = videoName(video);

  return (
    <article
      className={cn(
        "relative flex items-center gap-3 px-3 py-2 transition-colors",
        ready &&
          "hover:bg-muted/40 has-[a:focus-visible]:ring-3 has-[a:focus-visible]:ring-ring/50 has-[a:focus-visible]:ring-inset",
      )}
      data-testid="video-row"
      data-video-id={video.id}
    >
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        {/* One line: a name too long ends in "…" and shows whole on hover. */}
        <h2
          className="truncate font-heading text-base leading-snug font-medium"
          title={name}
          data-testid="video-name"
        >
          {ready ? (
            <Link
              to="/videos/$videoId"
              params={{ videoId: video.id }}
              // The whole row opens the video.
              className="outline-none after:absolute after:inset-0"
            >
              <VideoName video={video} />
            </Link>
          ) : (
            <VideoName video={video} />
          )}
        </h2>
        {(!ready || video.duration_ms != null) && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            {!ready && (
              <Badge variant={failure ? "destructive" : "secondary"}>
                {t(`library.status.${video.status}`)}
              </Badge>
            )}
            {video.duration_ms != null && (
              <span className="tabular-nums" data-testid="video-duration">
                {formatDuration(video.duration_ms)}
              </span>
            )}
          </div>
        )}
        <DownloadStatus video={video} className="max-w-sm" />
        {/* Above the row's link, for the failure's tooltip. */}
        <PreparationStatus video={video} className="relative max-w-sm" />
        {failure && !video.download && (
          <div className="relative flex items-center gap-2">
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
      </div>
      <VideoMenu video={video} />
      <div className="w-28 shrink-0 overflow-hidden rounded-md" data-testid="video-thumbnail">
        <VideoThumbnail video={video} />
      </div>
    </article>
  );
}

/** The song, then its artist and year, quieter: "Passwonky · Fats Waller · 1936". */
function VideoName({ video }: { video: Video }) {
  return (
    <>
      {video.title}
      {songDetails(video).map((detail) => (
        <span key={detail} className="font-normal text-muted-foreground">
          {" · "}
          {detail}
        </span>
      ))}
    </>
  );
}

/** "···": edit the song's details and tags, or delete the video. */
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
              className="relative shrink-0"
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
