/** Where "Go to" (`G`) can take you, parsed from what the user types. */
export type GoToTarget =
  | { readonly kind: "time"; readonly time: number }
  | { readonly kind: "bar"; readonly bar: number; readonly beat: number }
  | { readonly kind: "frame"; readonly frame: number };

const BAR = /^(?:b|bar|c|comp[aá]s)\s*(-?\d+)(?:[.:](\d+))?$/;
const FRAME = /^f\s*(\d+)$/;
const CLOCK = /^(\d+):(\d{1,2})(?::(\d{1,2}))?(?:[.,](\d+))?$/;
const SECONDS = /^(\d+(?:[.,]\d+)?)\s*s?$/;

/**
 * "1:23.5", "83.5" or "83 s" is a time; "b32", "b32.3", "c32" (compás) or "bar 32" a bar and
 * beat; "f740" a frame. Anything else is `null` (the user may be typing a marker's name).
 */
export function parseGoTo(input: string): GoToTarget | null {
  const text = input.trim().toLowerCase();
  const bar = BAR.exec(text);
  if (bar) {
    return { kind: "bar", bar: Number(bar[1]), beat: bar[2] ? Math.max(1, Number(bar[2])) : 1 };
  }
  const frame = FRAME.exec(text);
  if (frame) return { kind: "frame", frame: Number(frame[1]) };
  const clock = CLOCK.exec(text);
  if (clock) {
    const [, first = "0", second = "0", third, fraction] = clock;
    const seconds =
      third === undefined
        ? Number(first) * 60 + Number(second)
        : Number(first) * 3600 + Number(second) * 60 + Number(third);
    return { kind: "time", time: seconds + (fraction ? Number(`0.${fraction}`) : 0) };
  }
  const plain = SECONDS.exec(text);
  if (plain) return { kind: "time", time: Number((plain[1] ?? "0").replace(",", ".")) };
  return null;
}

/** Text compared ignoring case and accents. */
export function searchable(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}
