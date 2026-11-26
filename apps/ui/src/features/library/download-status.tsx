import type { Video } from "@segno/api-client";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

/** Progress of a YouTube download; nothing once the video is in the library. */
export function DownloadStatus({ video, className }: { video: Video; className?: string }) {
  const { t } = useTranslation();
  const download = video.download;
  if (!download) return null;
  const percent = Math.round(download.progress * 100);
  const label =
    download.stage === "downloading"
      ? t("download.downloading", { percent })
      : t(`download.${download.stage}`);
  return (
    <div className={cn("flex flex-col gap-1", className)} data-testid="download">
      <span className="text-xs text-muted-foreground">{label}</span>
      <div className="h-1 overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full rounded-full bg-primary transition-[width] duration-500",
            download.stage !== "downloading" && "animate-pulse",
          )}
          style={{ width: `${percent}%` }}
        />
      </div>
    </div>
  );
}
