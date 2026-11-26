import type { Video } from "@segno/api-client";
import { describe, expect, it } from "vitest";
import {
  allTags,
  filterVideos,
  parseTags,
  sortVideos,
  studyProgress,
  youtubeId,
} from "./library-filters";

function video(overrides: Partial<Video>): Video {
  return {
    id: "id",
    source: "file",
    title: "Untitled",
    status: "ready",
    seek_proxy: false,
    has_peaks: false,
    has_attacks: false,
    tags: [],
    created_at: "2026-11-20T10:00:00Z",
    updated_at: "2026-11-20T10:00:00Z",
    ...overrides,
  } as Video;
}

const MONK = video({
  id: "monk",
  title: "Thelonious Monk - I'm Confessin'",
  channel: "itsRemco",
  tags: ["Jazz", "To learn"],
  created_at: "2026-11-25T10:00:00Z",
  studied_at: "2026-11-25T12:00:00Z",
  duration_ms: 141_000,
  position_ms: 70_500,
});
const CHOPIN = video({
  id: "chopin",
  title: "Chopin - Nocturne Op. 9 Nº 2",
  tags: ["Clásica"],
  created_at: "2026-11-26T10:00:00Z",
});
const BACH = video({
  id: "bach",
  title: "bach - Prelude in C",
  created_at: "2026-11-24T10:00:00Z",
  studied_at: "2026-11-26T09:00:00Z",
});

describe("youtubeId", () => {
  it("reads every kind of YouTube link", () => {
    for (const link of [
      "https://www.youtube.com/watch?v=7A12z9S4F8U&list=RD7A12z9S4F8U&start_radio=1",
      "youtube.com/watch?feature=share&v=7A12z9S4F8U",
      "https://youtu.be/7A12z9S4F8U?t=42",
      "https://m.youtube.com/watch?v=7A12z9S4F8U",
      "https://music.youtube.com/watch?v=7A12z9S4F8U",
      "https://www.youtube.com/shorts/7A12z9S4F8U",
      "  https://www.youtube.com/live/7A12z9S4F8U  ",
    ]) {
      expect(youtubeId(link), link).toBe("7A12z9S4F8U");
    }
  });

  it("refuses anything else", () => {
    for (const text of [
      "https://vimeo.com/123",
      "https://www.youtube.com/playlist?list=RD7A12z9S4F8U",
      "https://www.youtube.com.evil.example/watch?v=7A12z9S4F8U",
      "watch this: https://youtu.be/7A12z9S4F8U",
      "7A12z9S4F8U",
      "",
    ]) {
      expect(youtubeId(text), text).toBeNull();
    }
  });
});

describe("filterVideos", () => {
  const all = [MONK, CHOPIN, BACH];

  it("finds words in titles, channels and tags, ignoring case and accents", () => {
    expect(filterVideos(all, "monk", null).map((v) => v.id)).toEqual(["monk"]);
    expect(filterVideos(all, "remco", null).map((v) => v.id)).toEqual(["monk"]);
    expect(filterVideos(all, "clasica", null).map((v) => v.id)).toEqual(["chopin"]);
    expect(filterVideos(all, "nocturne chopin", null).map((v) => v.id)).toEqual(["chopin"]);
    expect(filterVideos(all, "", null)).toHaveLength(3);
  });

  it("keeps only the videos with a tag", () => {
    expect(filterVideos(all, "", "jazz").map((v) => v.id)).toEqual(["monk"]);
  });
});

describe("sortVideos", () => {
  it("sorts by date added, last studied or title", () => {
    const all = [MONK, CHOPIN, BACH];
    expect(sortVideos(all, "added").map((v) => v.id)).toEqual(["chopin", "monk", "bach"]);
    expect(sortVideos(all, "studied").map((v) => v.id)).toEqual(["bach", "monk", "chopin"]);
    expect(sortVideos(all, "title").map((v) => v.id)).toEqual(["bach", "chopin", "monk"]);
  });
});

describe("tags", () => {
  it("are listed once, alphabetically", () => {
    const other = video({ tags: ["jazz", "Ballad"] });
    expect(allTags([MONK, CHOPIN, other])).toEqual(["Ballad", "Clásica", "Jazz", "To learn"]);
  });

  it("are typed separated by commas", () => {
    expect(parseTags(" Jazz, To learn ,, ")).toEqual(["Jazz", "To learn"]);
  });
});

describe("studyProgress", () => {
  it("is how far the user got", () => {
    expect(studyProgress(MONK)).toBe(0.5);
    expect(studyProgress(CHOPIN)).toBeNull();
  });
});
