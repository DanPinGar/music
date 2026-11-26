import { LoaderCircleIcon, TriangleAlertIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useCommandRegistry } from "@/commands/commands-context";
import { cn } from "@/lib/utils";
import { TOOL_LABELS } from "./tools-dialog";
import { isBusy, isUsable, useRefreshWhenToolsChange, useTools } from "./use-tools";

/**
 * The status bar's word on the external tools: what is being installed, what is missing or
 * failed, or an update waiting for the next start. Nothing when all is well. Opens the tools
 * dialog.
 */
export function ToolsStatus() {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const tools = useTools().data;
  useRefreshWhenToolsChange();
  if (!tools) return null;

  const busy = tools.tools.filter(isBusy);
  const missing = tools.tools.filter((tool) => !isUsable(tool) && !isBusy(tool));
  const failed = missing.some((tool) => tool.error);
  const pending = tools.tools.some((tool) => tool.pending_version);

  let content: { text: string; tone: string; icon?: "busy" | "warning" } | null = null;
  if (busy.length > 0) {
    const list = busy
      .map((tool) => {
        const percent = Math.round((tool.progress ?? 0) * 100);
        return `${TOOL_LABELS[tool.name]} ${percent >= 100 ? "✓" : `${percent} %`}`;
      })
      .join(" · ");
    content = { text: t("tools.statusInstalling", { list }), tone: "", icon: "busy" };
  } else if (failed) {
    content = { text: t("tools.statusFailed"), tone: "text-destructive", icon: "warning" };
  } else if (missing.length > 0) {
    const list = missing.map((tool) => TOOL_LABELS[tool.name]).join(", ");
    content = { text: t("tools.statusMissing", { list }), tone: "text-amber-500", icon: "warning" };
  } else if (pending) {
    content = { text: t("tools.statusPending"), tone: "" };
  }
  if (!content) return null;

  return (
    <button
      type="button"
      className={cn("flex items-center gap-1.5 hover:text-foreground", content.tone)}
      onClick={() => registry.execute("tools.show")}
      data-testid="tools-status"
    >
      {content.icon === "busy" && <LoaderCircleIcon className="size-3 animate-spin" />}
      {content.icon === "warning" && <TriangleAlertIcon className="size-3" />}
      {content.text}
    </button>
  );
}
