import { describe, expect, it } from "vitest";
import { NavigationHistory } from "./history";

describe("NavigationHistory", () => {
  it("goes back to where a jump started, and forward again", () => {
    const history = new NavigationHistory();
    history.noteSeek(10, 60, 0);
    expect(history.back(61)).toBe(10);
    expect(history.forward(10)).toBe(61);
    expect(history.canGoForward).toBe(false);
  });

  it("ignores steps", () => {
    const history = new NavigationHistory();
    history.noteSeek(10, 12, 0);
    history.noteSeek(12, 11.98, 2000);
    expect(history.canGoBack).toBe(false);
  });

  it("keeps one entry per burst of seeks, from where it started", () => {
    const history = new NavigationHistory();
    // Scrubbing: small seeks a few milliseconds apart, 30 s in total.
    for (let step = 0; step < 30; step++) history.noteSeek(10 + step, 11 + step, step * 50);
    expect(history.back(40)).toBe(10);
    expect(history.canGoBack).toBe(false);
  });

  it("starts a new burst after a pause", () => {
    const history = new NavigationHistory();
    history.noteSeek(10, 60, 0);
    history.noteSeek(60, 100, 5000);
    expect(history.back(100)).toBe(60);
    expect(history.back(60)).toBe(10);
  });

  it("forgets the forward entries after a new jump", () => {
    const history = new NavigationHistory();
    history.noteSeek(10, 60, 0);
    history.back(60);
    history.noteSeek(10, 90, 5000);
    expect(history.canGoForward).toBe(false);
  });
});
