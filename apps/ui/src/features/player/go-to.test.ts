import { describe, expect, it } from "vitest";
import { parseGoTo, searchable } from "./go-to";

describe("parseGoTo", () => {
  it("reads times", () => {
    expect(parseGoTo("1:23.5")).toEqual({ kind: "time", time: 83.5 });
    expect(parseGoTo("1:23,25")).toEqual({ kind: "time", time: 83.25 });
    expect(parseGoTo("1:02:03")).toEqual({ kind: "time", time: 3723 });
    expect(parseGoTo("83.5")).toEqual({ kind: "time", time: 83.5 });
    expect(parseGoTo(" 90 s ")).toEqual({ kind: "time", time: 90 });
  });

  it("reads bars, in English and Spanish", () => {
    expect(parseGoTo("b32")).toEqual({ kind: "bar", bar: 32, beat: 1 });
    expect(parseGoTo("B32.3")).toEqual({ kind: "bar", bar: 32, beat: 3 });
    expect(parseGoTo("bar 12")).toEqual({ kind: "bar", bar: 12, beat: 1 });
    expect(parseGoTo("c7")).toEqual({ kind: "bar", bar: 7, beat: 1 });
    expect(parseGoTo("compás 7")).toEqual({ kind: "bar", bar: 7, beat: 1 });
  });

  it("reads frames", () => {
    expect(parseGoTo("f740")).toEqual({ kind: "frame", frame: 740 });
  });

  it("leaves names alone", () => {
    expect(parseGoTo("chorus")).toBeNull();
    expect(parseGoTo("")).toBeNull();
  });
});

describe("searchable", () => {
  it("ignores case and accents", () => {
    expect(searchable("Puente Ñandú")).toBe("puente nandu");
  });
});
