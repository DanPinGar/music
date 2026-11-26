import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "zustand";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { barTime } from "./beat-grid";
import { type CommentItem, excerpt, filterComments } from "./comments";
import { parseGoTo, searchable } from "./go-to";
import type { MarkerItem } from "./markers";
import { formatTimecode } from "./navigation";
import type { PlayerRuntime } from "./player-runtime";

/** G: go to a time ("1:23.5"), a bar ("b32"), a frame ("f740") or a marker by name. */
export function GoToDialog({
  open,
  onOpenChange,
  runtime,
  markers,
  comments,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  runtime: PlayerRuntime;
  markers: readonly MarkerItem[];
  comments: readonly CommentItem[];
}) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const grid = useStore(runtime.session, (state) => state.grid);
  const target = parseGoTo(text);

  let destination: { time: number; label: string } | null = null;
  if (target?.kind === "time") {
    destination = {
      time: target.time,
      label: t("goTo.time", { time: formatTimecode(target.time, true) }),
    };
  } else if (target?.kind === "frame") {
    const time = (target.frame + 0.5) / runtime.fps;
    destination = { time, label: t("goTo.frame", { frame: target.frame }) };
  } else if (target?.kind === "bar" && grid) {
    destination = {
      time: barTime(grid, target.bar, target.beat),
      label: t("goTo.bar", { bar: target.bar, beat: target.beat }),
    };
  }
  const query = searchable(text.trim());
  const found = markers.filter(
    (marker) =>
      query === "" ||
      searchable(marker.label).includes(query) ||
      String(marker.number ?? "") === query,
  );
  // Comments only once something is typed: there can be many.
  const said = query === "" ? [] : filterComments(comments, text).slice(0, 20);

  const close = (open: boolean) => {
    if (!open) setText("");
    onOpenChange(open);
  };
  const go = (time: number) => {
    close(false);
    runtime.seek(time);
  };

  return (
    <CommandDialog
      open={open}
      onOpenChange={close}
      title={t("goTo.title")}
      description={t("goTo.description")}
    >
      <Command shouldFilter={false}>
        <CommandInput placeholder={t("goTo.placeholder")} value={text} onValueChange={setText} />
        <CommandList>
          <CommandEmpty>
            {target?.kind === "bar" && !grid ? t("goTo.needsGrid") : t("goTo.nothing")}
          </CommandEmpty>
          {destination && (
            <CommandGroup>
              <CommandItem value="destination" onSelect={() => destination && go(destination.time)}>
                {destination.label}
              </CommandItem>
            </CommandGroup>
          )}
          {found.length > 0 && (
            <CommandGroup heading={t("markers.title")}>
              {found.map((marker) => (
                <CommandItem key={marker.id} value={marker.id} onSelect={() => go(marker.start)}>
                  <span className="w-6 text-right font-mono text-xs text-muted-foreground">
                    {marker.number ?? "↻"}
                  </span>
                  <span className="flex-1 truncate">
                    {marker.label || t(`markers.kinds.${marker.kind}`)}
                  </span>
                  <span className="font-mono text-xs text-muted-foreground tabular-nums">
                    {formatTimecode(marker.start, true)}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {said.length > 0 && (
            <CommandGroup heading={t("comments.title")}>
              {said.map((comment) => (
                <CommandItem
                  key={comment.id}
                  value={`comment-${comment.id}`}
                  onSelect={() => go(comment.start)}
                >
                  <span className="flex-1 truncate">{excerpt(comment.body, 70)}</span>
                  <span className="font-mono text-xs text-muted-foreground tabular-nums">
                    {formatTimecode(comment.start, true)}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
