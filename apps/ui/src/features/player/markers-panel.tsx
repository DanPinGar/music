import { FlagIcon, PencilIcon, RepeatIcon, SquareIcon, Trash2Icon } from "lucide-react";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCommandRegistry } from "@/commands/commands-context";
import { Shortcut } from "@/commands/shortcut";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { DEFAULT_MARKER_COLOR, MARKER_COLORS, type MarkerItem } from "./markers";
import { formatTimecode } from "./navigation";
import type { PlayerRuntime } from "./player-runtime";
import type { MarkerActions } from "./use-markers";

const KIND_ICONS = { point: FlagIcon, section: SquareIcon, loop: RepeatIcon } as const;

/** Every marker, section and saved loop, to jump to, rename, recolour or delete. */
export function MarkersPanel({
  runtime,
  markers,
}: {
  runtime: PlayerRuntime;
  markers: MarkerActions;
}) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const [editing, setEditing] = useState<string | null>(null);

  const save = (item: MarkerItem, changes: Partial<Pick<MarkerItem, "label" | "color">>) => {
    const next = { ...item, ...changes };
    markers.change(item.id, {
      start_ms: Math.round(next.start * 1000),
      end_ms: next.end === null ? null : Math.round(next.end * 1000),
      label: next.label,
      color: next.color,
    });
  };
  const recolour = (item: MarkerItem) => {
    const index = MARKER_COLORS.indexOf((item.color ?? "") as (typeof MARKER_COLORS)[number]);
    save(item, { color: MARKER_COLORS[(index + 1) % MARKER_COLORS.length] ?? null });
  };
  const loopOver = (item: MarkerItem) => {
    if (item.end === null) return;
    runtime.session.setState({
      loop: { start: item.start, end: item.end, enabled: true },
      repetitions: 0,
    });
    runtime.seek(item.start);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="markers-panel">
      <div className="flex shrink-0 flex-col gap-1 border-b p-2">
        <Button
          variant="ghost"
          size="sm"
          className="justify-between"
          onClick={() => registry.execute("markers.add")}
        >
          {t("commands.addMarker")}
          <Shortcut binding="M" />
        </Button>
        <p className="px-2 text-xs text-muted-foreground">{t("markers.help")}</p>
      </div>
      <ul className="min-h-0 flex-1 overflow-y-auto p-1">
        {markers.items.length === 0 && (
          <li className="p-3 text-center text-xs text-muted-foreground">{t("markers.empty")}</li>
        )}
        {markers.items.map((item) => {
          const Icon = KIND_ICONS[item.kind];
          return (
            <li
              key={item.id}
              className="group flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-muted/50"
              data-testid="marker-row"
            >
              <button
                type="button"
                className="size-3 shrink-0 rounded-full ring-1 ring-foreground/20"
                style={{ background: item.color ?? DEFAULT_MARKER_COLOR }}
                aria-label={t("markers.recolour")}
                title={t("markers.recolour")}
                onClick={(event) => {
                  event.currentTarget.blur();
                  recolour(item);
                }}
              />
              <span className="w-4 shrink-0 text-right font-mono text-xs text-muted-foreground">
                {item.number ?? ""}
              </span>
              <Icon className="size-3.5 shrink-0 text-muted-foreground" />
              {editing === item.id ? (
                <input
                  // biome-ignore lint/a11y/noAutofocus: renaming starts typing at once.
                  autoFocus
                  className="h-6 min-w-0 flex-1 rounded bg-muted px-1.5 text-sm outline-none"
                  defaultValue={item.label}
                  aria-label={t("markers.rename")}
                  onBlur={(event) => {
                    setEditing(null);
                    if (event.currentTarget.value !== item.label) {
                      save(item, { label: event.currentTarget.value });
                    }
                  }}
                  onKeyDown={(event: KeyboardEvent<HTMLInputElement>) => {
                    if (event.key === "Enter") event.currentTarget.blur();
                    if (event.key === "Escape") {
                      event.currentTarget.value = item.label;
                      event.currentTarget.blur();
                    }
                  }}
                />
              ) : (
                <button
                  type="button"
                  className="min-w-0 flex-1 truncate text-left text-sm"
                  onClick={(event) => {
                    event.currentTarget.blur();
                    runtime.seek(item.start);
                  }}
                  onDoubleClick={() => setEditing(item.id)}
                  title={t("markers.jumpHint")}
                >
                  {item.label || (
                    <span className="text-muted-foreground">{t(`markers.kinds.${item.kind}`)}</span>
                  )}
                </button>
              )}
              <span className="shrink-0 font-mono text-[11px] text-muted-foreground tabular-nums">
                {formatTimecode(item.start)}
                {item.end !== null && `–${formatTimecode(item.end)}`}
              </span>
              <div
                className={cn(
                  "flex shrink-0 opacity-0 group-hover:opacity-100 focus-within:opacity-100",
                )}
              >
                {item.end !== null && (
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={t("markers.loopOver")}
                    title={t("markers.loopOver")}
                    onClick={(event) => {
                      event.currentTarget.blur();
                      loopOver(item);
                    }}
                  >
                    <RepeatIcon />
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("markers.rename")}
                  title={t("markers.rename")}
                  onClick={() => setEditing(item.id)}
                >
                  <PencilIcon />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("markers.delete")}
                  title={t("markers.delete")}
                  onClick={() => markers.remove(item.id)}
                >
                  <Trash2Icon />
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
