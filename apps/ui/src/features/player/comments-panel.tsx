import { PencilIcon, RepeatIcon, Trash2Icon } from "lucide-react";
import { type KeyboardEvent, memo, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "zustand";
import { useCommandRegistry } from "@/commands/commands-context";
import { Shortcut } from "@/commands/shortcut";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { CommentBody } from "./comment-body";
import { type CommentItem, covers, filterComments } from "./comments";
import { formatTimecode } from "./navigation";
import type { PlayerRuntime } from "./player-runtime";
import type { CommentActions } from "./use-comments";

/** A comment being written: new (`id` null) or an existing one being rewritten. */
export interface CommentDraft {
  readonly id: string | null;
  readonly start: number;
  readonly end: number | null;
  readonly body: string;
}

/** The playhead in steps of `step` seconds: re-renders a few times a second, not every frame. */
function useCoarseTime(runtime: PlayerRuntime, step: number): number {
  const [time, setTime] = useState(() => Math.floor(runtime.time() / step) * step);
  useEffect(() => {
    let frame = 0;
    const tick = () => {
      const coarse = Math.floor(runtime.time() / step) * step;
      setTime((previous) => (previous === coarse ? previous : coarse));
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [runtime, step]);
  return time;
}

function anchorLabel(start: number, end: number | null): string {
  return end === null
    ? formatTimecode(start, true)
    : `${formatTimecode(start, true)}–${formatTimecode(end, true)}`;
}

/** Comments: write one at the playhead (C), filter them, jump to them, rewrite or delete. */
export function CommentsPanel({
  runtime,
  comments,
  draft,
  onDraftChange,
}: {
  runtime: PlayerRuntime;
  comments: CommentActions;
  draft: CommentDraft | null;
  onDraftChange: (draft: CommentDraft | null) => void;
}) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const [filter, setFilter] = useState("");
  const time = useCoarseTime(runtime, 0.25);
  const shown = filterComments(comments.items, filter);

  const edit = (item: CommentItem) =>
    onDraftChange({ id: item.id, start: item.start, end: item.end, body: item.body });

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="comments-panel">
      <div className="flex shrink-0 flex-col gap-2 border-b p-2">
        {draft ? (
          <Composer
            runtime={runtime}
            draft={draft}
            onChange={onDraftChange}
            onSave={(saved) => {
              const fields = {
                start_ms: Math.round(saved.start * 1000),
                end_ms: saved.end === null ? null : Math.round(saved.end * 1000),
                body: saved.body,
              };
              if (saved.id) comments.change(saved.id, fields);
              else comments.add(fields);
              onDraftChange(null);
            }}
          />
        ) : (
          <Button
            variant="ghost"
            size="sm"
            className="justify-between"
            onClick={(event) => {
              event.currentTarget.blur();
              registry.execute("comments.add");
            }}
          >
            {t("commands.addComment")}
            <Shortcut binding="C" />
          </Button>
        )}
        {comments.items.length > 0 && (
          <input
            type="search"
            className="h-7 rounded-md bg-muted px-2 text-sm outline-none placeholder:text-muted-foreground"
            placeholder={t("comments.filter")}
            aria-label={t("comments.filter")}
            value={filter}
            onChange={(event) => setFilter(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setFilter("");
                event.currentTarget.blur();
              }
            }}
          />
        )}
      </div>
      <ul className="min-h-0 flex-1 overflow-y-auto p-1">
        {comments.items.length === 0 && (
          <li className="p-3 text-center text-xs text-muted-foreground">{t("comments.empty")}</li>
        )}
        {comments.items.length > 0 && shown.length === 0 && (
          <li className="p-3 text-center text-xs text-muted-foreground">
            {t("comments.noMatches")}
          </li>
        )}
        {shown.map((item) => (
          <CommentRow
            key={item.id}
            item={item}
            current={covers(item, time)}
            onJump={(at) => runtime.seek(at)}
            onEdit={() => edit(item)}
            onDelete={() => comments.remove(item.id)}
            onLoop={() => {
              if (item.end === null) return;
              runtime.session.setState({
                loop: { start: item.start, end: item.end, enabled: true },
                repetitions: 0,
              });
              runtime.seek(item.start);
            }}
          />
        ))}
      </ul>
    </div>
  );
}

const CommentRow = memo(function CommentRow({
  item,
  current,
  onJump,
  onEdit,
  onDelete,
  onLoop,
}: {
  item: CommentItem;
  current: boolean;
  onJump: (time: number) => void;
  onEdit: () => void;
  onDelete: () => void;
  onLoop: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li
      className={cn(
        "group flex flex-col gap-1 rounded-md border-l-2 border-transparent px-2 py-1.5 hover:bg-muted/50",
        current && "border-violet-400 bg-violet-500/10",
      )}
      data-testid="comment-row"
      data-current={current}
    >
      <div className="flex items-center gap-1">
        <button
          type="button"
          className="font-mono text-xs text-violet-300 tabular-nums hover:underline"
          onClick={(event) => {
            event.currentTarget.blur();
            onJump(item.start);
          }}
          title={t("comments.jump")}
        >
          {anchorLabel(item.start, item.end)}
        </button>
        <div className="ml-auto flex opacity-0 group-hover:opacity-100 focus-within:opacity-100">
          {item.end !== null && (
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={t("markers.loopOver")}
              title={t("markers.loopOver")}
              onClick={onLoop}
            >
              <RepeatIcon />
            </Button>
          )}
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={t("comments.edit")}
            title={t("comments.edit")}
            onClick={onEdit}
          >
            <PencilIcon />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={t("comments.delete")}
            title={t("comments.delete")}
            onClick={onDelete}
          >
            <Trash2Icon />
          </Button>
        </div>
      </div>
      <CommentBody body={item.body} onTime={onJump} />
    </li>
  );
});

/** The text box of a comment: Ctrl+Enter saves, Esc cancels and gives the keys back. */
function Composer({
  runtime,
  draft,
  onChange,
  onSave,
}: {
  runtime: PlayerRuntime;
  draft: CommentDraft;
  onChange: (draft: CommentDraft | null) => void;
  onSave: (draft: CommentDraft) => void;
}) {
  const { t } = useTranslation();
  const loop = useStore(runtime.session, (state) => state.loop);
  const box = useRef<HTMLTextAreaElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: focus again for every new draft.
  useEffect(() => box.current?.focus(), [draft.id, draft.start]);

  const save = () => {
    if (draft.body.trim() !== "") onSave(draft);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      save();
    } else if (event.key === "Escape") {
      event.preventDefault();
      onChange(null);
    }
  };

  return (
    <div className="flex flex-col gap-1.5" data-testid="comment-composer">
      <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
        <span>
          {draft.end === null
            ? t("comments.at", { time: formatTimecode(draft.start, true) })
            : t("comments.stretch", {
                start: formatTimecode(draft.start, true),
                end: formatTimecode(draft.end, true),
              })}
        </span>
        <span className="ml-auto flex gap-0.5">
          <Button
            variant="ghost"
            size="xs"
            onClick={() => onChange({ ...draft, start: runtime.time(), end: null })}
          >
            {t("comments.useMoment")}
          </Button>
          {loop && (
            <Button
              variant="ghost"
              size="xs"
              onClick={() => onChange({ ...draft, start: loop.start, end: loop.end })}
            >
              {t("comments.useLoop")}
            </Button>
          )}
        </span>
      </div>
      <textarea
        ref={box}
        rows={3}
        className="min-h-16 resize-y rounded-md bg-muted px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/50"
        placeholder={t("comments.placeholder")}
        aria-label={t("comments.placeholder")}
        value={draft.body}
        onChange={(event) => onChange({ ...draft, body: event.currentTarget.value })}
        onKeyDown={onKeyDown}
      />
      <div className="flex items-center justify-end gap-1">
        <span className="mr-auto text-[11px] text-muted-foreground">{t("comments.help")}</span>
        <Button variant="ghost" size="sm" onClick={() => onChange(null)}>
          {t("comments.cancel")}
        </Button>
        <Button size="sm" onClick={save} disabled={draft.body.trim() === ""}>
          {t("comments.save")}
        </Button>
      </div>
    </div>
  );
}
