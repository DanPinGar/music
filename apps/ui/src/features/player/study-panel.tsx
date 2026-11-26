import { XIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { type CommentDraft, CommentsPanel } from "./comments-panel";
import { MarkersPanel } from "./markers-panel";
import type { PlayerRuntime } from "./player-runtime";
import type { CommentActions } from "./use-comments";
import type { MarkerActions } from "./use-markers";

export type PanelTab = "markers" | "comments";

/** The side panel of the player: markers and comments, one tab each. */
export function StudyPanel({
  tab,
  onTabChange,
  onClose,
  runtime,
  markers,
  comments,
  draft,
  onDraftChange,
}: {
  tab: PanelTab;
  onTabChange: (tab: PanelTab) => void;
  onClose: () => void;
  runtime: PlayerRuntime;
  markers: MarkerActions;
  comments: CommentActions;
  draft: CommentDraft | null;
  onDraftChange: (draft: CommentDraft | null) => void;
}) {
  const { t } = useTranslation();
  const tabs: [PanelTab, string, number][] = [
    ["markers", t("markers.title"), markers.items.length],
    ["comments", t("comments.title"), comments.items.length],
  ];
  return (
    <aside className="flex w-80 shrink-0 flex-col border-l" data-testid="study-panel">
      <div className="flex h-10 shrink-0 items-center gap-1 border-b px-2" role="tablist">
        {tabs.map(([id, label, count]) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={cn(
              "rounded-md px-2.5 py-1 text-sm text-muted-foreground transition-colors hover:text-foreground",
              tab === id && "bg-muted text-foreground",
            )}
            onClick={(event) => {
              event.currentTarget.blur();
              onTabChange(id);
            }}
            data-testid={`panel-tab-${id}`}
          >
            {label} <span className="text-muted-foreground">{count}</span>
          </button>
        ))}
        <Button
          variant="ghost"
          size="icon-sm"
          className="ml-auto"
          aria-label={t("common.close")}
          onClick={onClose}
        >
          <XIcon />
        </Button>
      </div>
      {tab === "markers" ? (
        <MarkersPanel runtime={runtime} markers={markers} />
      ) : (
        <CommentsPanel
          runtime={runtime}
          comments={comments}
          draft={draft}
          onDraftChange={onDraftChange}
        />
      )}
    </aside>
  );
}
