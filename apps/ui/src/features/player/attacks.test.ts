import { describe, expect, it } from "vitest";
import { firstAtOrAfter, nextAttack, previousAttack } from "./attacks";

const TIMES = [0.5, 1.25, 2, 2.75];

describe("nextAttack", () => {
  it("finds the attack after the playhead", () => {
    expect(nextAttack(TIMES, 0)).toBe(0.5);
    expect(nextAttack(TIMES, 1)).toBe(1.25);
  });

  it("moves on from an attack the playhead stands on", () => {
    expect(nextAttack(TIMES, 1.25)).toBe(2);
  });

  it("has nothing after the last attack", () => {
    expect(nextAttack(TIMES, 2.75)).toBeUndefined();
    expect(nextAttack([], 0)).toBeUndefined();
  });
});

describe("previousAttack", () => {
  it("finds the attack before the playhead", () => {
    expect(previousAttack(TIMES, 1.5)).toBe(1.25);
    expect(previousAttack(TIMES, 3)).toBe(2.75);
  });

  it("moves on from an attack the playhead stands on", () => {
    expect(previousAttack(TIMES, 2)).toBe(1.25);
  });

  it("skips the attack just heard while playing", () => {
    expect(previousAttack(TIMES, 2.1)).toBe(2);
    expect(previousAttack(TIMES, 2.1, true)).toBe(1.25);
  });

  it("has nothing before the first attack", () => {
    expect(previousAttack(TIMES, 0.5)).toBeUndefined();
  });
});

describe("firstAtOrAfter", () => {
  it("finds where a time would go", () => {
    expect(firstAtOrAfter(TIMES, 0)).toBe(0);
    expect(firstAtOrAfter(TIMES, 1.25)).toBe(1);
    expect(firstAtOrAfter(TIMES, 1.3)).toBe(2);
    expect(firstAtOrAfter(TIMES, 9)).toBe(4);
  });
});
