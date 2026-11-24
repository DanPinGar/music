import { describe, expect, it, vi } from "vitest";
import { type Command, CommandRegistry } from "./registry";

function command(id: string, overrides: Partial<Command> = {}): Command {
  return { id, title: "commands.showShortcuts", group: "general", run: () => {}, ...overrides };
}

describe("CommandRegistry", () => {
  it("lists commands in registration order", () => {
    const registry = new CommandRegistry();
    registry.register(command("a"));
    registry.register(command("b"));
    expect(registry.list().map((c) => c.id)).toEqual(["a", "b"]);
  });

  it("refuses duplicate ids", () => {
    const registry = new CommandRegistry();
    registry.register(command("a"));
    expect(() => registry.register(command("a"))).toThrow(/already registered/);
  });

  it("removes commands and notifies subscribers", () => {
    const registry = new CommandRegistry();
    const listener = vi.fn();
    registry.subscribe(listener);
    const remove = registry.register(command("a"));
    remove();
    expect(registry.get("a")).toBeUndefined();
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("keeps the same snapshot until something changes", () => {
    const registry = new CommandRegistry();
    registry.register(command("a"));
    expect(registry.list()).toBe(registry.list());
  });

  it("finds commands by binding, whatever way the binding is written", () => {
    const registry = new CommandRegistry();
    registry.register(command("palette.open", { keys: ["Ctrl+K"] }));
    expect(registry.findByBinding("ctrl+k")?.id).toBe("palette.open");
    expect(registry.findByBinding("K")).toBeUndefined();
  });

  it("skips disabled commands", () => {
    const registry = new CommandRegistry();
    const run = vi.fn();
    registry.register(command("a", { keys: ["Space"], enabled: () => false, run }));
    expect(registry.findByBinding("Space")).toBeUndefined();
    expect(registry.execute("a")).toBe(false);
    expect(run).not.toHaveBeenCalled();
  });

  it("executes enabled commands", () => {
    const registry = new CommandRegistry();
    const run = vi.fn();
    registry.register(command("a", { run }));
    expect(registry.execute("a")).toBe(true);
    expect(registry.execute("missing")).toBe(false);
    expect(run).toHaveBeenCalledOnce();
  });
});
