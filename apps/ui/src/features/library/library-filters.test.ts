import type { Video } from "@segno/api-client";
import { describe, expect, it } from "vitest";
import {
  allTags,
  filterVideos,
  parseTags,
  parseYear,
  sortVideos,
  videoName,
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
  title: "I'm Confessin'",
  artist: "Thelonious Monk",
  year: 1965,
  channel: "itsRemco",
  tags: ["Jazz", "To learn"],
  created_at: "2026-11-25T10:00:00Z",
  studied_at: "2026-11-25T12:00:00Z",
  duration_ms: 141_000,
  position_ms: 70_500,
});
const CHOPIN = video({
  id: "chopin",
  title: "Nocturne Op. 9 Nº 2",
  artist: "Chopin",
  year: 1832,
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
  it("sorts by year, date added, last studied or title", () => {
    const all = [MONK, CHOPIN, BACH];
    expect(sortVideos(all, "year").map((v) => v.id)).toEqual(["chopin", "monk", "bach"]);
    expect(sortVideos(all, "added").map((v) => v.id)).toEqual(["chopin", "monk", "bach"]);
    expect(sortVideos(all, "studied").map((v) => v.id)).toEqual(["bach", "monk", "chopin"]);
    expect(sortVideos(all, "title").map((v) => v.id)).toEqual(["bach", "monk", "chopin"]);
  });

  it("puts songs of the same year by artist, and those without a year last", () => {
    const waller = video({ id: "waller", title: "Passwonky", artist: "Fats Waller", year: 1936 });
    const ellington = video({ id: "ellington", title: "Caravan", artist: "Ellington", year: 1936 });
    const unknown = video({ id: "unknown", title: "Another song" });
    expect(sortVideos([unknown, waller, ellington], "year").map((v) => v.id)).toEqual([
      "ellington",
      "waller",
      "unknown",
    ]);
  });
});

describe("videoName", () => {
  it("is the song, then its artist and year, when known", () => {
    expect(videoName(MONK)).toBe("I'm Confessin' · Thelonious Monk · 1965");
    expect(videoName(BACH)).toBe("bach - Prelude in C");
    expect(videoName(video({ title: "Clair de Lune", year: 1905 }))).toBe("Clair de Lune · 1905");
  });

  it("is what the search looks in", () => {
    expect(filterVideos([MONK, CHOPIN, BACH], "thelonious 1965", null).map((v) => v.id)).toEqual([
      "monk",
    ]);
  });
});

describe("parseYear", () => {
  it("reads a year, or nothing", () => {
    expect(parseYear(" 1936 ")).toBe(1936);
    expect(parseYear("")).toBeNull();
    for (const wrong of ["0", "19365", "1936a", "-5"])
      expect(parseYear(wrong), wrong).toBeUndefined();
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
