import type { Video } from "@segno/api-client";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

/** Progress (or failure) of preparing a video for instant navigation; nothing once ready. */
export function PreparationStatus({ video, className }: { video: Video; className?: string }) {
  const { t } = useTranslation();
  if (video.preparation) {
    const percent = Math.round(video.preparation.progress * 100);
    return (
      <div className={cn("flex flex-col gap-1", className)} data-testid="preparation">
        <span className="text-xs text-muted-foreground">
          {video.preparation.stage === "queued"
            ? t("preparation.queued")
            : t("preparation.running", { percent })}
        </span>
        <div className="h-1 overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-primary transition-[width] duration-500"
            style={{ width: `${percent}%` }}
          />
        </div>
      </div>
    );
  }
  if (video.preparation_error) {
    return (
      <span className={cn("text-xs text-amber-500", className)} title={video.preparation_error}>
        {t("preparation.failed")}
      </span>
    );
  }
  return null;
}
