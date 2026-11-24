import { describe, expect, it } from "vitest";
import { bindingFromEvent, type KeyInput, normalizeBinding, splitBinding } from "./keymap";

function press(key: string, code: string, modifiers: Partial<KeyInput> = {}): string | null {
  return bindingFromEvent({
    key,
    code,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...modifiers,
  });
}

describe("bindingFromEvent", () => {
  it("uses the printed character for letters, with modifiers first", () => {
    expect(press("k", "KeyK", { ctrlKey: true })).toBe("Ctrl+K");
    expect(press("A", "KeyA", { shiftKey: true })).toBe("Shift+A");
    expect(press("k", "KeyK", { ctrlKey: true, altKey: true })).toBe("Ctrl+Alt+K");
    expect(press("ñ", "Semicolon")).toBe("Ñ");
  });

  it("names special keys", () => {
    expect(press(" ", "Space")).toBe("Space");
    expect(press("ArrowLeft", "ArrowLeft", { shiftKey: true })).toBe("Shift+ArrowLeft");
    expect(press("Backspace", "Backspace")).toBe("Backspace");
  });

  it("ignores Shift for symbols, so they match on every layout", () => {
    // Spanish layout: "?" is Shift + the key right of 0; "+" needs no Shift.
    expect(press("?", "Minus", { shiftKey: true })).toBe("?");
    expect(press("+", "BracketRight")).toBe("+");
    // US layout: "+" is Shift + "=".
    expect(press("+", "Equal", { shiftKey: true })).toBe("+");
    expect(press(",", "Comma")).toBe(",");
  });

  it("treats AltGr (Ctrl+Alt on Windows) as part of typing a symbol", () => {
    expect(press("[", "BracketLeft", { ctrlKey: true, altKey: true })).toBe("[");
  });

  it("reads top-row digits from the physical key and numpad keys from what they produce", () => {
    expect(press("&", "Digit1")).toBe("1"); // AZERTY without Shift
    expect(press("!", "Digit1", { shiftKey: true })).toBe("Shift+1");
    expect(press("1", "Numpad1")).toBe("1");
    expect(press("End", "Numpad1")).toBe("End"); // Num Lock off
  });

  it("returns null for a lone modifier", () => {
    expect(press("Shift", "ShiftLeft", { shiftKey: true })).toBeNull();
    expect(press("Control", "ControlLeft", { ctrlKey: true })).toBeNull();
  });
});

describe("normalizeBinding", () => {
  it("produces the same strings as key presses", () => {
    expect(normalizeBinding("ctrl+k")).toBe("Ctrl+K");
    expect(normalizeBinding("shift+arrowleft")).toBe("Shift+ArrowLeft");
    expect(normalizeBinding("Shift+?")).toBe("?");
    expect(normalizeBinding("space")).toBe("Space");
    expect(normalizeBinding("alt+shift+ctrl+x")).toBe("Ctrl+Alt+Shift+X");
    expect(normalizeBinding("Ctrl++")).toBe("Ctrl++");
    expect(normalizeBinding("+")).toBe("+");
    expect(normalizeBinding("Shift+1")).toBe("Shift+1");
  });

  it("rejects unknown modifiers and empty keys", () => {
    expect(() => normalizeBinding("Hyper+K")).toThrow(/Unknown modifier/);
    expect(() => normalizeBinding("Ctrl+")).toThrow(/no key/);
  });
});

describe("splitBinding", () => {
  it("keeps a literal plus key", () => {
    expect(splitBinding("Ctrl++")).toEqual({ modifiers: ["Ctrl"], key: "+" });
    expect(splitBinding("Shift+ArrowLeft")).toEqual({ modifiers: ["Shift"], key: "ArrowLeft" });
  });
});
