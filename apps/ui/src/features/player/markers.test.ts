import type { Marker } from "@segno/api-client";
import { describe, expect, it } from "vitest";
import {
  markerByNumber,
  nextMarkerTime,
  nextSectionColor,
  previousMarkerTime,
  sectionAt,
  toMarkerItems,
} from "./markers";

function marker(id: string, kind: Marker["kind"], start_ms: number, end_ms?: number): Marker {
  return {
    id,
    kind,
    start_ms,
    end_ms: end_ms ?? null,
    label: "",
    color: null,
    created_at: "",
    updated_at: "",
  };
}

const ITEMS = toMarkerItems([
  marker("c", "point", 50_000),
  marker("a", "section", 10_000, 40_000),
  marker("l", "loop", 12_000, 14_000),
  marker("b", "point", 20_000),
]);

describe("toMarkerItems", () => {
  it("orders markers in time and numbers points and sections", () => {
    expect(ITEMS.map((item) => [item.id, item.number])).toEqual([
      ["a", 1],
      ["l", null],
      ["b", 2],
      ["c", 3],
    ]);
    expect(ITEMS[0]?.end).toBe(40);
  });
});

describe("marker navigation", () => {
  it("goes to the next and previous stops, skipping saved loops", () => {
    expect(nextMarkerTime(ITEMS, 10)).toBe(20);
    expect(previousMarkerTime(ITEMS, 20)).toBe(10);
    expect(previousMarkerTime(ITEMS, 20.1, true)).toBe(10);
    expect(nextMarkerTime(ITEMS, 50)).toBeUndefined();
  });

  it("finds markers by number", () => {
    expect(markerByNumber(ITEMS, 2)?.id).toBe("b");
    expect(markerByNumber(ITEMS, 9)).toBeUndefined();
  });
});

describe("sectionAt", () => {
  it("finds the section playing", () => {
    expect(sectionAt(ITEMS, 25)?.id).toBe("a");
    expect(sectionAt(ITEMS, 45)).toBeUndefined();
  });
});

describe("nextSectionColor", () => {
  it("gives each new section the next colour", () => {
    expect(nextSectionColor([])).not.toBe(nextSectionColor(ITEMS));
  });
});
