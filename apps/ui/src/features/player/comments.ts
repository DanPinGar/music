import type { Comment } from "@segno/api-client";
import { searchable } from "./go-to";

/** A comment in seconds. */
export interface CommentItem {
  readonly id: string;
  readonly start: number;
  /** End of the stretch commented on; `null` for a moment. */
  readonly end: number | null;
  /** Markdown. */
  readonly body: string;
}

/** Comments on a moment stay "current" this long after it, while playing past it. */
const MOMENT_SECONDS = 3;
/** Comments are drawn in this colour on the timeline. */
export const COMMENT_COLOR = "#a78bfa";

export function toCommentItems(comments: readonly Comment[]): CommentItem[] {
  return [...comments]
    .sort((a, b) => a.start_ms - b.start_ms || a.id.localeCompare(b.id))
    .map((comment) => ({
      id: comment.id,
      start: comment.start_ms / 1000,
      end: comment.end_ms == null ? null : comment.end_ms / 1000,
      body: comment.body,
    }));
}

/** Whether `comment` speaks about `time`: inside its stretch, or just after its moment. */
export function covers(comment: CommentItem, time: number): boolean {
  const end = comment.end ?? comment.start + MOMENT_SECONDS;
  return time >= comment.start - 0.05 && time < end;
}

/** The comments about `time`, the latest first. */
export function commentsAt(items: readonly CommentItem[], time: number): CommentItem[] {
  return items.filter((item) => covers(item, time)).reverse();
}

/** Comments whose text contains `query` (ignoring case and accents). */
export function filterComments(items: readonly CommentItem[], query: string): CommentItem[] {
  const wanted = searchable(query.trim());
  if (wanted === "") return [...items];
  return items.filter((item) => searchable(item.body).includes(wanted));
}

/** A comment's first line as plain text, shortened to `length` characters. */
export function excerpt(body: string, length = 90): string {
  const firstLine = body.split("\n").find((line) => line.trim() !== "") ?? "";
  const plain = firstLine
    .replace(/^\s*(?:[-*+]|\d+\.|#+|>)\s+/, "")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[*_~`]/g, "")
    .trim();
  return plain.length > length ? `${plain.slice(0, length - 1)}…` : plain;
}
