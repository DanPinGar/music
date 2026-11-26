import type { Link, Parent, PhrasingContent, Root, Text } from "mdast";
import { parseGoTo } from "./go-to";

/** "1:23", "1:23.5", "12:05,25" — not inside longer numbers such as "10:30:45" or "1.2:30". */
const TIME = /(?<![\d:.,])(\d{1,2}:\d{2}(?:[.,]\d{1,3})?)(?![\d:])/g;
/** Links to a time in the video: `#t=83.5`. */
export const TIME_LINK = "#t=";

/** The time a `#t=` link points at, or `null` for any other link. */
export function linkTime(href: string | undefined): number | null {
  if (!href?.startsWith(TIME_LINK)) return null;
  const time = Number(href.slice(TIME_LINK.length));
  return Number.isFinite(time) ? time : null;
}

/** Splits a text into plain stretches and `#t=` links for the times written in it. */
export function splitTimes(value: string): PhrasingContent[] {
  const parts: PhrasingContent[] = [];
  let last = 0;
  for (const match of value.matchAll(TIME)) {
    const written = match[1] ?? "";
    const target = parseGoTo(written);
    if (target?.kind !== "time") continue;
    const index = match.index ?? 0;
    if (index > last) parts.push({ type: "text", value: value.slice(last, index) } satisfies Text);
    parts.push({
      type: "link",
      url: `${TIME_LINK}${target.time}`,
      children: [{ type: "text", value: written }],
    } satisfies Link);
    last = index + written.length;
  }
  if (last < value.length) parts.push({ type: "text", value: value.slice(last) } satisfies Text);
  return parts;
}

/** Remark plugin: times written in a comment ("see 1:23.5") become links to that moment. */
export function remarkTimes() {
  return (tree: Root) => {
    visit(tree);
  };
}

function visit(node: Parent): void {
  const children: Parent["children"] = [];
  for (const child of node.children) {
    if (child.type === "text") {
      children.push(...splitTimes(child.value));
    } else {
      // Not inside links (their text is already a link) or code.
      if ("children" in child && child.type !== "link" && child.type !== "linkReference") {
        visit(child);
      }
      children.push(child);
    }
  }
  node.children = children;
}
