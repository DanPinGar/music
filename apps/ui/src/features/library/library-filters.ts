import type { Video } from "@segno/api-client";
import { searchable } from "@/features/player/go-to";

/** How the library is sorted. */
export type LibraryOrder = "year" | "added" | "studied" | "title";
export const LIBRARY_ORDERS: readonly LibraryOrder[] = ["year", "added", "studied", "title"];
export const DEFAULT_ORDER: LibraryOrder = "year";

/** What follows the song's title in its name: the artist, then the year. */
export function songDetails(video: Video): string[] {
  return [video.artist, video.year?.toString()].filter((part): part is string => !!part);
}

/** How the library names a video: the song, the artist, the year ("Passwonky · Fats Waller · 1936"). */
export function videoName(video: Video): string {
  return [video.title, ...songDetails(video)].join(" · ");
}

const VIDEO_ID = /^[A-Za-z0-9_-]{11}$/;

/**
 * The id of a YouTube video in an address (watch, youtu.be, shorts, live, embed, music or
 * mobile links), or `null`. The same rules as the core's, to answer before asking it.
 */
export function youtubeId(text: string): string | null {
  const trimmed = text.trim();
  if (trimmed === "" || /\s/.test(trimmed)) return null;
  let url: URL;
  try {
    url = new URL(/^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`);
  } catch {
    return null;
  }
  const host = url.hostname.toLowerCase().replace(/^www\./, "");
  const path = url.pathname.replace(/^\//, "");
  let candidate: string | undefined;
  if (host === "youtu.be") {
    candidate = path.split("/")[0];
  } else if (["youtube.com", "m.youtube.com", "music.youtube.com"].includes(host)) {
    if (path === "watch") {
      candidate = url.searchParams.get("v") ?? undefined;
    } else {
      const match = /^(?:shorts|live|embed)\/([^/]+)/.exec(path);
      candidate = match?.[1];
    }
  }
  return candidate && VIDEO_ID.test(candidate) ? candidate : null;
}

/** Videos whose song, artist, year, channel or tags contain `query`, and that carry `tag` if given. */
export function filterVideos(videos: readonly Video[], query: string, tag: string | null): Video[] {
  const wanted = searchable(query.trim());
  return videos.filter((video) => {
    if (tag !== null && !video.tags.some((own) => own.toLowerCase() === tag.toLowerCase())) {
      return false;
    }
    if (wanted === "") return true;
    const haystack = searchable([videoName(video), video.channel ?? "", ...video.tags].join(" "));
    return wanted.split(/\s+/).every((word) => haystack.includes(word));
  });
}

/**
 * Oldest song first (those without a year last, then by artist and title), newest added
 * first, last studied first (never studied last), or by title.
 */
export function sortVideos(videos: readonly Video[], order: LibraryOrder): Video[] {
  const sorted = [...videos];
  const added = (a: Video, b: Video) => b.created_at.localeCompare(a.created_at);
  const text = (a?: string | null, b?: string | null) =>
    (a ?? "").localeCompare(b ?? "", undefined, { sensitivity: "base" });
  if (order === "year") {
    sorted.sort(
      (a, b) =>
        (a.year ?? Number.POSITIVE_INFINITY) - (b.year ?? Number.POSITIVE_INFINITY) ||
        text(a.artist, b.artist) ||
        text(a.title, b.title) ||
        added(a, b),
    );
  } else if (order === "title") {
    sorted.sort((a, b) => text(a.title, b.title));
  } else if (order === "studied") {
    sorted.sort((a, b) => (b.studied_at ?? "").localeCompare(a.studied_at ?? "") || added(a, b));
  } else {
    sorted.sort(added);
  }
  return sorted;
}

/** Every tag in the library once (ignoring case), in alphabetical order. */
export function allTags(videos: readonly Video[]): string[] {
  const tags = new Map<string, string>();
  for (const video of videos) {
    for (const tag of video.tags) {
      if (!tags.has(tag.toLowerCase())) tags.set(tag.toLowerCase(), tag);
    }
  }
  return [...tags.values()].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
}

/** How far into the video the user got, from 0 to 1, or `null` if never opened. */
export function studyProgress(video: Video): number | null {
  if (video.position_ms == null || !video.duration_ms) return null;
  return Math.min(1, Math.max(0, video.position_ms / video.duration_ms));
}

/** Tags typed as "Jazz, To learn ,": split on commas, trimmed, without empty ones. */
export function parseTags(text: string): string[] {
  return text
    .split(",")
    .map((tag) => tag.trim())
    .filter((tag) => tag !== "");
}

/** The year typed as a song's year: `null` when empty, `undefined` when not a year. */
export function parseYear(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  if (!/^\d{1,4}$/.test(trimmed)) return undefined;
  const year = Number(trimmed);
  return year >= 1 ? year : undefined;
}
