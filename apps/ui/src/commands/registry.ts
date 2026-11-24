import type { ParseKeys } from "i18next";
import { normalizeBinding } from "./keymap";

export const COMMAND_GROUPS = ["general", "navigation", "playback", "loops", "markers"] as const;
export type CommandGroup = (typeof COMMAND_GROUPS)[number];

export interface Command {
  /** Stable identifier used by keymaps, e.g. "palette.open". */
  readonly id: string;
  /** Translation key of the title shown in the palette and in the shortcuts list. */
  readonly title: ParseKeys;
  readonly group: CommandGroup;
  /** Default key bindings (see keymap.ts), e.g. ["Ctrl+K"]. */
  readonly keys?: readonly string[];
  /** Keep out of the command palette (the command still has its shortcuts). */
  readonly hiddenInPalette?: boolean;
  /** Whether the command can run right now. Shortcuts skip disabled commands. */
  readonly enabled?: () => boolean;
  readonly run: () => void;
}

type Listener = () => void;

/**
 * The single source of every user action.
 *
 * Keyboard shortcuts, the command palette, menus, page-turner pedals and MIDI-learn
 * bindings all resolve to commands registered here, so each action is reachable in every
 * way and user keymaps are just data.
 */
export class CommandRegistry {
  readonly #commands = new Map<string, Command>();
  readonly #bindings = new Map<string, readonly string[]>();
  readonly #listeners = new Set<Listener>();
  #snapshot: readonly Command[] = [];

  /** Adds a command; returns a function that removes it again. */
  register(command: Command): () => void {
    if (this.#commands.has(command.id)) {
      throw new Error(`Command "${command.id}" is already registered`);
    }
    this.#commands.set(command.id, command);
    this.#bindings.set(command.id, (command.keys ?? []).map(normalizeBinding));
    this.#changed();
    return () => {
      if (this.#commands.get(command.id) !== command) return;
      this.#commands.delete(command.id);
      this.#bindings.delete(command.id);
      this.#changed();
    };
  }

  registerAll(commands: readonly Command[]): () => void {
    const removers = commands.map((command) => this.register(command));
    return () => {
      for (const remove of removers) remove();
    };
  }

  get(id: string): Command | undefined {
    return this.#commands.get(id);
  }

  /** Registered commands in registration order (a stable snapshot between changes). */
  list(): readonly Command[] {
    return this.#snapshot;
  }

  bindingsOf(id: string): readonly string[] {
    return this.#bindings.get(id) ?? [];
  }

  /** The enabled command bound to a key press, if any. */
  findByBinding(binding: string): Command | undefined {
    const wanted = normalizeBinding(binding);
    for (const command of this.#commands.values()) {
      if (this.bindingsOf(command.id).includes(wanted) && isEnabled(command)) return command;
    }
    return undefined;
  }

  /** Runs a command; returns false when it does not exist or is disabled. */
  execute(id: string): boolean {
    const command = this.#commands.get(id);
    if (!command || !isEnabled(command)) return false;
    command.run();
    return true;
  }

  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  #changed(): void {
    this.#snapshot = [...this.#commands.values()];
    for (const listener of this.#listeners) listener();
  }
}

function isEnabled(command: Command): boolean {
  return command.enabled?.() ?? true;
}
