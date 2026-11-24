import { useTranslation } from "react-i18next";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { splitBinding } from "./keymap";

const KEY_LABELS: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Escape: "Esc",
  Backspace: "⌫",
  Enter: "↵",
  Meta: "⌘",
};

/** A key binding drawn as keycaps, e.g. "Ctrl+K" → [Ctrl] [K]. */
export function Shortcut({ binding, className }: { binding: string; className?: string }) {
  const { t } = useTranslation();
  const { modifiers, key } = splitBinding(binding);
  const label = (part: string) => (part === "Space" ? t("keys.space") : (KEY_LABELS[part] ?? part));
  return (
    <KbdGroup className={className}>
      {[...modifiers, key].map((part, index) => (
        // Parts of one binding never reorder, so the index is a stable key.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        <Kbd key={index}>{label(part)}</Kbd>
      ))}
    </KbdGroup>
  );
}
