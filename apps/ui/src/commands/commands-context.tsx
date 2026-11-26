import { createContext, type ReactNode, useContext, useEffect, useSyncExternalStore } from "react";
import { bindingFromEvent } from "./keymap";
import type { Command, CommandRegistry } from "./registry";

const CommandRegistryContext = createContext<CommandRegistry | null>(null);

export function CommandRegistryProvider({
  registry,
  children,
}: {
  registry: CommandRegistry;
  children: ReactNode;
}) {
  return <CommandRegistryContext value={registry}>{children}</CommandRegistryContext>;
}

export function useCommandRegistry(): CommandRegistry {
  const registry = useContext(CommandRegistryContext);
  if (!registry)
    throw new Error("useCommandRegistry must be used inside <CommandRegistryProvider>");
  return registry;
}

/** Every registered command, re-rendering when commands come and go. */
export function useCommandList(): readonly Command[] {
  const registry = useCommandRegistry();
  return useSyncExternalStore(
    (listener) => registry.subscribe(listener),
    () => registry.list(),
  );
}

/** Registers commands while the calling component is mounted. Pass a memoised array. */
export function useCommands(commands: readonly Command[]): void {
  const registry = useCommandRegistry();
  useEffect(() => registry.registerAll(commands), [registry, commands]);
}

/**
 * Runs the command bound to each key press, except while the user is typing or a dialog is
 * open (or still opening or closing: keys typed then belong to the dialog, not the player).
 */
export function useGlobalShortcuts(): void {
  const registry = useCommandRegistry();
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || event.isComposing) return;
      const binding = bindingFromEvent(event);
      if (!binding) return;
      if (binding !== "Escape" && (isTypingTarget(event.target) || dialogOpen())) return;
      const command = registry.findByBinding(binding);
      if (!command) return;
      event.preventDefault();
      command.run({ binding });
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [registry]);
}

const NON_TEXT_INPUTS = new Set(["button", "checkbox", "radio", "range", "reset", "submit"]);

/** Whether a dialog is open, or still opening or closing. */
export function dialogOpen(): boolean {
  return document.querySelector("[data-slot=dialog-content]") !== null;
}

/** Whether keys pressed in `target` are text being typed. */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (target instanceof HTMLInputElement) return !NON_TEXT_INPUTS.has(target.type);
  return false;
}
