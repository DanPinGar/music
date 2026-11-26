import type { Marker } from "@segno/api-client";

/** A marker in seconds, numbered as the keys 1–9 reach it. */
export interface MarkerItem {
  readonly id: string;
  readonly kind: Marker["kind"];
  readonly start: number;
  readonly end: number | null;
  readonly label: string;
  readonly color: string | null;
  /** 1, 2, 3… for points and sections in time order; `null` for saved loops. */
  readonly number: number | null;
}

/** Colours offered for sections and markers: distinct on dark and light backgrounds. */
export const MARKER_COLORS = [
  "#e5484d",
  "#f76b15",
  "#ffc53d",
  "#46a758",
  "#12a594",
  "#0090ff",
  "#8e4ec6",
  "#d6409f",
] as const;

/** Points without a colour of their own. */
export const DEFAULT_MARKER_COLOR = "#ffc53d";
const SAME_TIME = 0.001;
const PLAYING_GRACE = 0.25;

export function toMarkerItems(markers: readonly Marker[]): MarkerItem[] {
  const sorted = [...markers].sort((a, b) => a.start_ms - b.start_ms || a.id.localeCompare(b.id));
  let number = 0;
  return sorted.map((marker) => ({
    id: marker.id,
    kind: marker.kind,
    start: marker.start_ms / 1000,
    end: marker.end_ms == null ? null : marker.end_ms / 1000,
    label: marker.label,
    color: marker.color ?? null,
    number: marker.kind === "loop" ? null : ++number,
  }));
}

/** Points and section starts: where Ctrl+←/→ and the number keys go. */
function stops(items: readonly MarkerItem[]): number[] {
  return items.filter((item) => item.kind !== "loop").map((item) => item.start);
}

export function nextMarkerTime(items: readonly MarkerItem[], time: number): number | undefined {
  return stops(items).find((start) => start > time + SAME_TIME);
}

export function previousMarkerTime(
  items: readonly MarkerItem[],
  time: number,
  playing = false,
): number | undefined {
  const limit = time - (playing ? PLAYING_GRACE : SAME_TIME);
  return stops(items)
    .filter((start) => start < limit)
    .at(-1);
}

export function markerByNumber(
  items: readonly MarkerItem[],
  number: number,
): MarkerItem | undefined {
  return items.find((item) => item.number === number);
}

/** The section playing at `time` (the latest to start, if they overlap). */
export function sectionAt(items: readonly MarkerItem[], time: number): MarkerItem | undefined {
  return items
    .filter((item) => item.kind === "section" && item.start <= time && (item.end ?? 0) > time)
    .at(-1);
}

/** A colour for a new section, different from the last ones used. */
export function nextSectionColor(items: readonly MarkerItem[]): string {
  const sections = items.filter((item) => item.kind === "section").length;
  return MARKER_COLORS[sections % MARKER_COLORS.length] ?? DEFAULT_MARKER_COLOR;
}

/** Every time the timeline snaps to: markers' starts and ends. */
export function markerTimes(items: readonly MarkerItem[]): number[] {
  const times: number[] = [];
  for (const item of items) {
    times.push(item.start);
    if (item.end !== null) times.push(item.end);
  }
  return times.sort((a, b) => a - b);
}
