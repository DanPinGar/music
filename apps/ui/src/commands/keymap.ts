/**
 * Keyboard bindings written as strings: "Space", "Ctrl+K", "Shift+ArrowLeft", "?", "-", "Ctrl++".
 *
 * The rules make shortcuts work on any keyboard layout, Spanish included:
 * - Letters use the character printed on the key ("K"), so they follow the user's layout.
 * - Digits on the top row use the physical key ("1"), so they also work where digits need
 *   Shift (AZERTY). Numpad keys use what they produce ("1", or "End" without Num Lock).
 * - Other printable symbols ("?", "+", "-", ",", ".") use the produced character and ignore
 *   Shift, because Shift is just part of how each layout types them. AltGr, which Windows
 *   reports as Ctrl+Alt, is ignored the same way.
 * - Named keys use their `KeyboardEvent.key` name ("ArrowLeft", "Home", "Backspace").
 * - Modifiers always come first, in the order Ctrl, Alt, Shift, Meta.
 */

export type KeyInput = Pick<
  KeyboardEvent,
  "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

export const MODIFIERS = ["Ctrl", "Alt", "Shift", "Meta"] as const;
export type Modifier = (typeof MODIFIERS)[number];

const MODIFIER_KEYS = new Set(["Control", "Alt", "AltGraph", "Shift", "Meta", "OS"]);

const NAMED_KEYS = [
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Backspace",
  "Delete",
  "Insert",
  "Enter",
  "Escape",
  "Tab",
  "Space",
  ...Array.from({ length: 12 }, (_, i) => `F${i + 1}`),
];
const NAMED_KEYS_BY_LOWERCASE = new Map(NAMED_KEYS.map((name) => [name.toLowerCase(), name]));

const KEY_ALIASES: Record<string, string> = {
  " ": "Space",
  Spacebar: "Space",
  Esc: "Escape",
  Left: "ArrowLeft",
  Right: "ArrowRight",
  Up: "ArrowUp",
  Down: "ArrowDown",
};

const LETTER = /^\p{L}$/u;
const TOP_ROW_DIGIT = /^Digit(\d)$/;

/** The binding produced by a key press, or `null` for a lone modifier. */
export function bindingFromEvent(event: KeyInput): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null;

  let key: string;
  let symbol = false;
  const digit = TOP_ROW_DIGIT.exec(event.code)?.[1];
  if (digit !== undefined) {
    key = digit;
  } else if (event.key.length === 1 && event.key !== " ") {
    if (LETTER.test(event.key)) {
      key = event.key.toUpperCase();
    } else {
      key = event.key;
      symbol = true;
    }
  } else {
    key = KEY_ALIASES[event.key] ?? event.key;
  }

  const altGraph = symbol && event.ctrlKey && event.altKey;
  const modifiers = new Set<Modifier>();
  if (event.ctrlKey && !altGraph) modifiers.add("Ctrl");
  if (event.altKey && !altGraph) modifiers.add("Alt");
  if (event.shiftKey && !symbol) modifiers.add("Shift");
  if (event.metaKey) modifiers.add("Meta");
  return format(modifiers, key);
}

/** Splits a binding into modifiers and key, keeping a literal "+" key: "Ctrl++" → Ctrl, +. */
export function splitBinding(binding: string): { modifiers: string[]; key: string } {
  if (binding === "+") return { modifiers: [], key: "+" };
  if (binding.endsWith("++")) {
    return { modifiers: binding.slice(0, -2).split("+"), key: "+" };
  }
  const parts = binding.split("+");
  const key = parts.pop() ?? "";
  return { modifiers: parts, key };
}

/** Canonical form of a binding written by hand or stored in settings: "ctrl+k" → "Ctrl+K". */
export function normalizeBinding(binding: string): string {
  const parts = splitBinding(binding.trim());
  const modifiers = new Set<Modifier>();
  for (const part of parts.modifiers) {
    const modifier = MODIFIERS.find((m) => m.toLowerCase() === part.trim().toLowerCase());
    if (!modifier) throw new Error(`Unknown modifier "${part}" in binding "${binding}"`);
    modifiers.add(modifier);
  }

  const raw = parts.key === " " ? " " : parts.key.trim();
  if (raw === "") throw new Error(`Binding "${binding}" has no key`);
  let key: string;
  let symbol = false;
  if (raw.length === 1 && raw !== " ") {
    if (LETTER.test(raw)) {
      key = raw.toUpperCase();
    } else {
      key = raw;
      symbol = !/\d/.test(raw);
    }
  } else {
    key = KEY_ALIASES[raw] ?? NAMED_KEYS_BY_LOWERCASE.get(raw.toLowerCase()) ?? raw;
  }
  if (symbol) modifiers.delete("Shift");
  return format(modifiers, key);
}

function format(modifiers: ReadonlySet<Modifier>, key: string): string {
  return [...MODIFIERS.filter((m) => modifiers.has(m)), key].join("+");
}
