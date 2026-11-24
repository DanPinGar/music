import { useTranslation } from "react-i18next";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command";
import { useCommandList, useCommandRegistry } from "./commands-context";
import { groupCommands } from "./group-commands";
import { Shortcut } from "./shortcut";

/** Ctrl+K: search every command by name and run it. */
export function CommandPalette({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const commands = useCommandList().filter((command) => !command.hiddenInPalette);

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("palette.title")}
      description={t("palette.description")}
    >
      <Command>
        <CommandInput placeholder={t("palette.placeholder")} />
        <CommandList>
          <CommandEmpty>{t("palette.empty")}</CommandEmpty>
          {groupCommands(commands).map(([group, items]) => (
            <CommandGroup key={group} heading={t(`commandGroups.${group}`)}>
              {items.map((command) => {
                const binding = registry.bindingsOf(command.id)[0];
                return (
                  <CommandItem
                    key={command.id}
                    value={command.id}
                    keywords={[t(command.title)]}
                    disabled={command.enabled?.() === false}
                    onSelect={() => {
                      onOpenChange(false);
                      registry.execute(command.id);
                    }}
                  >
                    {t(command.title)}
                    {binding && (
                      <CommandShortcut>
                        <Shortcut binding={binding} />
                      </CommandShortcut>
                    )}
                  </CommandItem>
                );
              })}
            </CommandGroup>
          ))}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
