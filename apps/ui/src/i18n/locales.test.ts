import { describe, expect, it } from "vitest";
import en from "./locales/en.json";
import es from "./locales/es.json";

function keys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) =>
    keys(child, prefix ? `${prefix}.${key}` : key),
  );
}

describe("translations", () => {
  it("have the same keys in every language", () => {
    expect(keys(es).sort()).toEqual(keys(en).sort());
  });

  it("have no empty texts", () => {
    for (const locale of [en, es]) {
      const texts = JSON.stringify(locale).match(/"[^"]*":""/g);
      expect(texts).toBeNull();
    }
  });
});
