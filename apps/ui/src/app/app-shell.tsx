import { Link, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { FileVideoIcon, KeyboardIcon, LanguagesIcon, SearchIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { usePlatform } from "@/api/api-context";
import { CommandPalette } from "@/commands/command-palette";
import { useCommandRegistry, useCommands, useGlobalShortcuts } from "@/commands/commands-context";
import type { Command } from "@/commands/registry";
import { Shortcut } from "@/commands/shortcut";
import { ShortcutsDialog } from "@/commands/shortcuts-dialog";
import { SegnoMark } from "@/components/segno-mark";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useImportVideos } from "@/features/library/use-import-videos";
import { currentLanguage, setLanguage } from "@/i18n";
import { StatusBar } from "./status-bar";

const NAV_LINK =
  "rounded-md px-2.5 py-1 text-sm text-muted-foreground transition-colors hover:text-foreground";
const NAV_LINK_ACTIVE = "bg-muted text-foreground";

/** Window layout: top bar, the current screen and the status bar. Owns app-wide commands. */
export function AppShell() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const registry = useCommandRegistry();
  const platform = usePlatform();
  const { importFiles, pickAndImport } = useImportVideos();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [dragging, setDragging] = useState(false);
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const inPlayer = pathname === "/player" || pathname.startsWith("/videos/");

  // Videos dropped anywhere on the window are imported.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void platform
      .watchFileDrops({
        over: () => setDragging(true),
        leave: () => setDragging(false),
        drop: (paths) => {
          setDragging(false);
          void importFiles(paths);
        },
      })
      .then((unwatch) => {
        if (cancelled) unwatch();
        else stop = unwatch;
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [platform, importFiles]);

  useGlobalShortcuts();
  const commands = useMemo<Command[]>(
    () => [
      {
        id: "library.import",
        title: "commands.importVideo",
        group: "general",
        keys: ["Ctrl+O"],
        run: () => void pickAndImport(),
      },
      {
        id: "palette.open",
        title: "commands.openPalette",
        group: "general",
        keys: ["Ctrl+K"],
        hiddenInPalette: true,
        run: () => setPaletteOpen(true),
      },
      {
        id: "shortcuts.show",
        title: "commands.showShortcuts",
        group: "general",
        keys: ["?"],
        run: () => setShortcutsOpen(true),
      },
      {
        id: "language.toggle",
        title: "commands.toggleLanguage",
        group: "general",
        run: () => setLanguage(currentLanguage() === "es" ? "en" : "es"),
      },
      {
        id: "nav.library",
        title: "commands.goToLibrary",
        group: "navigation",
        keys: ["Ctrl+1"],
        run: () => void navigate({ to: "/" }),
      },
      {
        id: "nav.player",
        title: "commands.goToPlayer",
        group: "navigation",
        keys: ["Ctrl+2"],
        run: () => void navigate({ to: "/player" }),
      },
    ],
    [navigate, pickAndImport],
  );
  useCommands(commands);

  return (
    <div className="flex h-svh flex-col">
      <header className="flex h-12 shrink-0 items-center gap-4 border-b px-4">
        <div className="flex items-center gap-2 font-heading font-semibold">
          <SegnoMark className="size-5 text-primary" />
          {t("app.name")}
        </div>
        <nav className="flex items-center gap-1">
          <Link
            to="/"
            className={NAV_LINK}
            activeProps={{ className: NAV_LINK_ACTIVE }}
            activeOptions={{ exact: true }}
          >
            {t("nav.library")}
          </Link>
          <Link
            to="/player"
            className={`${NAV_LINK} ${inPlayer ? NAV_LINK_ACTIVE : ""}`}
            aria-current={inPlayer ? "page" : undefined}
          >
            {t("nav.player")}
          </Link>
        </nav>
        <div className="ml-auto flex items-center gap-1">
          <Button
            variant="outline"
            size="sm"
            className="gap-2 text-muted-foreground"
            onClick={() => registry.execute("palette.open")}
          >
            <SearchIcon />
            {t("palette.open")}
            <Shortcut binding="Ctrl+K" />
          </Button>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("commands.showShortcuts")}
                  onClick={() => registry.execute("shortcuts.show")}
                />
              }
            >
              <KeyboardIcon />
            </TooltipTrigger>
            <TooltipContent>{t("commands.showShortcuts")}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={t("language.label")}
                  onClick={() => registry.execute("language.toggle")}
                />
              }
            >
              <LanguagesIcon />
              {t("language.other")}
            </TooltipTrigger>
            <TooltipContent>{t("commands.toggleLanguage")}</TooltipContent>
          </Tooltip>
        </div>
      </header>

      <main className="relative min-h-0 flex-1 overflow-y-auto">
        <Outlet />
        {dragging && (
          <div className="pointer-events-none absolute inset-3 z-40 flex flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed border-primary/60 bg-background/80 text-lg font-medium backdrop-blur-sm">
            <FileVideoIcon className="size-10 text-primary" />
            {t("library.dropHere")}
          </div>
        )}
      </main>

      <StatusBar />
      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
      <ShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
    </div>
  );
}
