import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useCommandList, useCommandRegistry } from "./commands-context";
import { groupCommands } from "./group-commands";
import { Shortcut } from "./shortcut";

/** "?": every command with its shortcuts, grouped. */
export function ShortcutsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const commands = useCommandList();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("shortcuts.title")}</DialogTitle>
          <DialogDescription>{t("shortcuts.description")}</DialogDescription>
        </DialogHeader>
        <div className="flex max-h-[60vh] flex-col gap-5 overflow-y-auto">
          {groupCommands(commands).map(([group, items]) => (
            <section key={group}>
              <h3 className="mb-2 text-xs font-medium text-muted-foreground">
                {t(`commandGroups.${group}`)}
              </h3>
              <ul className="flex flex-col gap-1.5">
                {items.map((command) => {
                  const bindings = registry.bindingsOf(command.id);
                  return (
                    <li key={command.id} className="flex items-center justify-between gap-4">
                      <span>{t(command.title)}</span>
                      <span className="flex gap-2">
                        {bindings.length > 0 ? (
                          bindings.map((binding) => <Shortcut key={binding} binding={binding} />)
                        ) : (
                          <span className="text-xs text-muted-foreground">
                            {t("shortcuts.none")}
                          </span>
                        )}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
