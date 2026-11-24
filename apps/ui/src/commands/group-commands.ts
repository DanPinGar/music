import { COMMAND_GROUPS, type Command, type CommandGroup } from "./registry";

/** Commands grouped in the canonical group order, skipping empty groups. */
export function groupCommands(commands: readonly Command[]): [CommandGroup, Command[]][] {
  return COMMAND_GROUPS.map((group): [CommandGroup, Command[]] => [
    group,
    commands.filter((c) => c.group === group),
  ]).filter(([, items]) => items.length > 0);
}
