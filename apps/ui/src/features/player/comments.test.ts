import type { Comment } from "@segno/api-client";
import { describe, expect, it } from "vitest";
import { commentsAt, excerpt, filterComments, toCommentItems } from "./comments";
import { linkTime, splitTimes } from "./remark-times";

function comment(id: string, start_ms: number, end_ms: number | null, body: string): Comment {
  return { id, start_ms, end_ms, body, created_at: "", updated_at: "" };
}

const ITEMS = toCommentItems([
  comment("b", 30_000, null, "**Pedal** aquí"),
  comment("a", 10_000, 20_000, "Mano izquierda sola"),
]);

describe("comments", () => {
  it("are ordered in time, in seconds", () => {
    expect(ITEMS.map((item) => [item.id, item.start, item.end])).toEqual([
      ["a", 10, 20],
      ["b", 30, null],
    ]);
  });

  it("speak about their stretch, or a few seconds after their moment", () => {
    expect(commentsAt(ITEMS, 15).map((item) => item.id)).toEqual(["a"]);
    expect(commentsAt(ITEMS, 31).map((item) => item.id)).toEqual(["b"]);
    expect(commentsAt(ITEMS, 25)).toEqual([]);
    expect(commentsAt(ITEMS, 40)).toEqual([]);
  });

  it("filter by their text, ignoring case and accents", () => {
    expect(filterComments(ITEMS, "AQUI").map((item) => item.id)).toEqual(["b"]);
    expect(filterComments(ITEMS, "  ")).toHaveLength(2);
  });

  it("make a plain excerpt of their first line", () => {
    expect(excerpt("\n- **Pedal** on [beat 2](x)\nmore")).toBe("Pedal on beat 2");
    expect(excerpt("a".repeat(100), 10)).toBe("aaaaaaaaa…");
  });
});

describe("times in comments", () => {
  it("become links to the moment", () => {
    const parts = splitTimes("Ver 1:23.5 y luego 2:05, no 10:30:45");
    expect(parts.map((part) => part.type)).toEqual(["text", "link", "text", "link", "text"]);
    const links = parts.filter((part) => part.type === "link");
    expect(links.map((link) => linkTime(link.url))).toEqual([83.5, 125]);
  });

  it("leave other text and links alone", () => {
    expect(splitTimes("no times here")).toEqual([{ type: "text", value: "no times here" }]);
    expect(linkTime("https://example.com")).toBeNull();
  });
});
