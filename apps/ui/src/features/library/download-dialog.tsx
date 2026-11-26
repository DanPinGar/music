import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { isUsable, useTools } from "@/features/tools/use-tools";
import { youtubeId } from "./library-filters";
import { useLibraryActions } from "./use-library-actions";

/** Ctrl+D: paste a YouTube link and download the video into the library. */
export function DownloadDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const { download } = useLibraryActions();
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const valid = youtubeId(url) !== null;
  // Downloads need yt-dlp, Deno and ffmpeg, which Segno may still be installing.
  const tools = useTools().data?.tools;
  const ready = !tools || tools.every(isUsable);

  const close = (next: boolean) => {
    if (!next) setUrl("");
    onOpenChange(next);
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!valid || busy) return;
    setBusy(true);
    const video = await download(url);
    setBusy(false);
    if (video) close(false);
  };

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={(event) => void submit(event)} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{t("library.downloadTitle")}</DialogTitle>
            <DialogDescription>{t("library.downloadDescription")}</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1.5">
            <Input
              autoFocus
              value={url}
              onChange={(event) => setUrl(event.currentTarget.value)}
              placeholder="https://www.youtube.com/watch?v=…"
              aria-label={t("library.downloadUrl")}
              aria-invalid={url.trim() !== "" && !valid}
              data-testid="download-url"
            />
            {url.trim() !== "" && !valid && (
              <p className="text-xs text-destructive">{t("library.downloadInvalid")}</p>
            )}
          </div>
          <p className="text-xs text-muted-foreground">{t("library.downloadNotice")}</p>
          {!ready && (
            <p className="text-xs text-amber-500" data-testid="download-waiting">
              {t("library.downloadWaiting")}
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => close(false)}>
              {t("comments.cancel")}
            </Button>
            <Button type="submit" disabled={!valid || busy || !ready} data-testid="download-start">
              {t("library.downloadStart")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
