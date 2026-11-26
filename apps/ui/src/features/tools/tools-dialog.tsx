import type { Tool, ToolName } from "@segno/api-client";
import { RefreshCwIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { currentLanguage } from "@/i18n";
import { cn } from "@/lib/utils";
import { isBusy, useCheckTools, useTools } from "./use-tools";

/** How each tool is written. */
export const TOOL_LABELS: Record<ToolName, string> = {
  ffmpeg: "ffmpeg",
  "yt-dlp": "yt-dlp",
  deno: "Deno",
};

/** The external tools: versions, installs and updates, and a button to look for updates. */
export function ToolsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const tools = useTools();
  const check = useCheckTools();
  const managed = tools.data?.managed ?? false;
  const busy = tools.data?.tools.some(isBusy) ?? false;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("tools.title")}</DialogTitle>
          <DialogDescription>
            {managed ? t("tools.description") : t("tools.notManaged")}
          </DialogDescription>
        </DialogHeader>
        <ul className="flex flex-col divide-y" data-testid="tools">
          {tools.data?.tools.map((tool) => (
            <ToolRow key={tool.name} tool={tool} />
          ))}
        </ul>
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("common.close")}
          </Button>
          {managed && (
            <Button
              onClick={() => check.mutate()}
              disabled={check.isPending || busy}
              data-testid="tools-check"
            >
              <RefreshCwIcon className={cn(busy && "animate-spin")} />
              {busy ? t("tools.checking") : t("tools.check")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ToolRow({ tool }: { tool: Tool }) {
  const { t } = useTranslation();
  const percent = Math.round((tool.progress ?? 0) * 100);
  const label =
    isBusy(tool) && percent >= 100
      ? t("tools.state.finishing")
      : tool.state === "installing" || tool.state === "updating"
        ? t(`tools.state.${tool.state}`, { percent })
        : t(`tools.state.${tool.state}`);
  const notes: string[] = [];
  if (tool.state === "external") notes.push(t("tools.external"));
  if (tool.state === "unsupported") notes.push(t("tools.unsupported"));
  if (tool.pending_version) notes.push(t("tools.pending", { version: tool.pending_version }));
  if (tool.checked_at) {
    const date = new Date(tool.checked_at).toLocaleString(currentLanguage(), {
      dateStyle: "medium",
      timeStyle: "short",
    });
    notes.push(t("tools.checked", { date }));
  }

  return (
    <li className="flex flex-col gap-1.5 py-3" data-testid={`tool-${tool.name}`}>
      <div className="flex items-baseline gap-2">
        <span className="font-medium">{TOOL_LABELS[tool.name]}</span>
        <span
          className="truncate font-mono text-xs text-muted-foreground"
          title={tool.version ?? ""}
        >
          {tool.version}
        </span>
        <Badge
          variant={tool.state === "missing" ? "destructive" : "secondary"}
          className="ml-auto shrink-0"
        >
          {label}
        </Badge>
      </div>
      <span className="text-xs text-muted-foreground">{t(`tools.purpose.${tool.name}`)}</span>
      {isBusy(tool) && (
        <div className="h-1 overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-primary transition-[width] duration-500"
            style={{ width: `${percent}%` }}
          />
        </div>
      )}
      {notes.map((note) => (
        <span key={note} className="text-xs text-muted-foreground">
          {note}
        </span>
      ))}
      {tool.error && (
        <span className="text-xs break-words text-destructive">
          {t("tools.error", { error: tool.error })}
        </span>
      )}
    </li>
  );
}
