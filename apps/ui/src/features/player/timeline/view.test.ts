import { describe, expect, it } from "vitest";
import {
  clampView,
  fitView,
  followView,
  MIN_SPAN,
  panView,
  revealView,
  ruler,
  rulerLabel,
  timeAt,
  xAt,
  zoomView,
} from "./view";

const SONG = 141;

describe("clampView", () => {
  it("keeps the view inside the song", () => {
    expect(clampView({ start: -3, span: 10 }, SONG)).toEqual({ start: 0, span: 10 });
    expect(clampView({ start: 138, span: 10 }, SONG)).toEqual({ start: 131, span: 10 });
  });

  it("limits the span to between half a second and the song", () => {
    expect(clampView({ start: 5, span: 0.1 }, SONG).span).toBe(MIN_SPAN);
    expect(clampView({ start: 5, span: 500 }, SONG)).toEqual({ start: 0, span: SONG });
    expect(clampView({ start: 0, span: 10 }, 0.3)).toEqual({ start: 0, span: 0.3 });
  });

  it("copes with an unknown duration", () => {
    expect(clampView({ start: 4, span: 10 }, Number.NaN)).toEqual({ start: 0, span: 10 });
  });
});

describe("zoomView", () => {
  it("keeps the anchor time under the cursor", () => {
    const view = { start: 20, span: 10 };
    const zoomed = zoomView(view, 0.5, 24, SONG);
    expect(zoomed.span).toBe(5);
    // 24 s was 40 % into the view and stays there.
    expect((24 - zoomed.start) / zoomed.span).toBeCloseTo(0.4);
  });

  it("stops at the limits", () => {
    expect(zoomView({ start: 20, span: 1 }, 0.1, 20.5, SONG).span).toBe(MIN_SPAN);
    expect(zoomView({ start: 20, span: 100 }, 4, 50, SONG)).toEqual(fitView(SONG));
  });
});

describe("panView", () => {
  it("moves the view without leaving the song", () => {
    expect(panView({ start: 20, span: 10 }, 5, SONG)).toEqual({ start: 25, span: 10 });
    expect(panView({ start: 2, span: 10 }, -5, SONG)).toEqual({ start: 0, span: 10 });
  });
});

describe("followView", () => {
  const view = { start: 20, span: 10 };

  it("turns the page when the playhead leaves it", () => {
    expect(followView(view, 25, "page", SONG)).toBe(view);
    expect(followView(view, 30.1, "page", SONG)).toEqual({ start: 29.6, span: 10 });
    expect(followView(view, 3, "page", SONG)).toEqual({ start: 2.5, span: 10 });
  });

  it("keeps the playhead centred when scrolling continuously", () => {
    expect(followView(view, 40, "continuous", SONG)).toEqual({ start: 35, span: 10 });
    expect(followView(view, 2, "continuous", SONG)).toEqual({ start: 0, span: 10 });
  });

  it("stays put when off, but jumps still reveal the playhead", () => {
    expect(followView(view, 80, "off", SONG)).toBe(view);
    expect(revealView(view, 80, "off", SONG)).toEqual({ start: 79.5, span: 10 });
    expect(revealView(view, 22, "off", SONG)).toBe(view);
  });
});

describe("coordinates", () => {
  it("map times to pixels and back", () => {
    const view = { start: 10, span: 20 };
    expect(xAt(view, 15, 800)).toBe(200);
    expect(timeAt(view, 200, 800)).toBe(15);
  });
});

describe("ruler", () => {
  it("picks the finest step whose labels fit", () => {
    expect(ruler({ start: 0, span: 10 }, 1000, 80).step).toBe(1);
    expect(ruler({ start: 0, span: 141 }, 1000, 80).step).toBe(15);
    expect(ruler({ start: 0, span: 1 }, 1000, 80).step).toBe(0.1);
  });

  it("labels the ticks inside the view", () => {
    const { labels, minor } = ruler({ start: 9.5, span: 3 }, 600, 150);
    expect(labels).toEqual([10, 11, 12]);
    expect(minor).toBe(0.2);
  });

  it("avoids floating-point noise in fine steps", () => {
    const { labels } = ruler({ start: 0.25, span: 0.3 }, 1000, 60);
    expect(labels).toContain(0.3);
    expect(labels.every((time) => String(time).length <= 5)).toBe(true);
  });
});

describe("rulerLabel", () => {
  it("shows as many decimals as the step needs", () => {
    expect(rulerLabel(65, 5)).toBe("1:05");
    expect(rulerLabel(65.5, 0.5)).toBe("1:05.5");
    expect(rulerLabel(65.25, 0.05)).toBe("1:05.25");
    expect(rulerLabel(0.3, 0.1)).toBe("0:00.3");
  });
});
