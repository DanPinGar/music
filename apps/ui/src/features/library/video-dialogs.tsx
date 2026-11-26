import type { Video } from "@segno/api-client";
import { type FormEvent, useId, useState } from "react";
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
import { cn } from "@/lib/utils";
import { parseTags, parseYear, videoName } from "./library-filters";
import { useLibraryActions } from "./use-library-actions";

interface VideoDialogProps {
  readonly video: Video;
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
}

/** Edits the song a video plays (title, artist, year) and replaces its tags. */
export function EditVideoDialog({ video, open, onOpenChange }: VideoDialogProps) {
  const { t } = useTranslation();
  const { edit } = useLibraryActions();
  const [title, setTitle] = useState(video.title);
  const [artist, setArtist] = useState(video.artist ?? "");
  const [year, setYear] = useState(video.year?.toString() ?? "");
  const [tags, setTags] = useState(video.tags.join(", "));
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      // Start from what the video has now, not from an earlier, abandoned edit.
      setTitle(video.title);
      setArtist(video.artist ?? "");
      setYear(video.year?.toString() ?? "");
      setTags(video.tags.join(", "));
    }
  }
  const parsedYear = parseYear(year);
  const valid = title.trim() !== "" && parsedYear !== undefined;
  const id = useId();

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    onOpenChange(false);
    await edit(video, {
      title: title.trim(),
      artist: artist.trim() || null,
      year: parsedYear,
      tags: parseTags(tags),
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form onSubmit={(event) => void submit(event)} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{t("library.editTitle")}</DialogTitle>
            <DialogDescription>{t("library.editDescription")}</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1.5">
            <label htmlFor={`${id}-title`}>{t("library.editName")}</label>
            <Input
              id={`${id}-title`}
              autoFocus
              value={title}
              maxLength={300}
              onChange={(event) => setTitle(event.currentTarget.value)}
              data-testid="edit-title"
            />
          </div>
          <div className="flex gap-3">
            <div className="flex min-w-0 flex-1 flex-col gap-1.5">
              <label htmlFor={`${id}-artist`}>{t("library.editArtist")}</label>
              <Input
                id={`${id}-artist`}
                value={artist}
                maxLength={200}
                onChange={(event) => setArtist(event.currentTarget.value)}
                data-testid="edit-artist"
              />
            </div>
            <div className="flex w-24 flex-col gap-1.5">
              <label htmlFor={`${id}-year`}>{t("library.editYear")}</label>
              <Input
                id={`${id}-year`}
                value={year}
                inputMode="numeric"
                maxLength={4}
                placeholder="1936"
                aria-invalid={parsedYear === undefined}
                aria-describedby={`${id}-year-hint`}
                onChange={(event) => setYear(event.currentTarget.value)}
                data-testid="edit-year"
              />
            </div>
          </div>
          <span
            id={`${id}-year-hint`}
            className={cn(
              "-mt-2 text-xs text-muted-foreground",
              parsedYear === undefined && "text-destructive",
            )}
          >
            {parsedYear === undefined ? t("library.editYearInvalid") : t("library.editYearHint")}
          </span>
          <div className="flex flex-col gap-1.5">
            <label htmlFor={`${id}-tags`}>{t("library.editTags")}</label>
            <Input
              id={`${id}-tags`}
              value={tags}
              onChange={(event) => setTags(event.currentTarget.value)}
              data-testid="edit-tags"
            />
            <span className="text-xs text-muted-foreground">{t("library.editTagsHint")}</span>
          </div>
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              {t("comments.cancel")}
            </Button>
            <Button type="submit" disabled={!valid} data-testid="edit-save">
              {t("library.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** Asks before deleting a video, which takes its files, markers and comments with it. */
export function DeleteVideoDialog({ video, open, onOpenChange }: VideoDialogProps) {
  const { t } = useTranslation();
  const { remove } = useLibraryActions();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("library.deleteTitle", { title: videoName(video) })}</DialogTitle>
          <DialogDescription>{t("library.deleteBody")}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("comments.cancel")}
          </Button>
          <Button
            variant="destructive"
            autoFocus
            onClick={() => {
              onOpenChange(false);
              void remove(video);
            }}
            data-testid="delete-confirm"
          >
            {t("library.deleteConfirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
