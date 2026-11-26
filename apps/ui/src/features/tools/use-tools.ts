import { expectData, type Tool, type ToolName, type Tools } from "@segno/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useApi } from "@/api/api-context";

export const TOOL_NAMES: readonly ToolName[] = ["ffmpeg", "yt-dlp", "deno"];

/** Whether the core is downloading the tool right now. */
export function isBusy(tool: Tool): boolean {
  return tool.state === "installing" || tool.state === "updating";
}

/** Whether what needs the tool works: it is installed, or found on the system. */
export function isUsable(tool: Tool | undefined): boolean {
  return tool?.version != null;
}

/** The external tools (ffmpeg, yt-dlp, Deno), followed every second while one downloads. */
export function useTools() {
  const api = useApi();
  return useQuery({
    queryKey: ["tools"],
    queryFn: async () => expectData(await api.GET("/tools")),
    refetchInterval: (query) => (query.state.data?.tools.some(isBusy) ? 1_000 : 60_000),
  });
}

/**
 * When a tool becomes usable (or changes version), fetches again what depends on it: the
 * versions in the status bar, and the videos the core now prepares. Mount it once.
 */
export function useRefreshWhenToolsChange(): void {
  const queryClient = useQueryClient();
  const tools = useTools().data;
  const usable = useRef<string | null>(null);
  useEffect(() => {
    if (!tools) return;
    const now = usableKey(tools);
    if (usable.current !== null && usable.current !== now) {
      void queryClient.invalidateQueries({ queryKey: ["system"] });
      void queryClient.invalidateQueries({ queryKey: ["videos"] });
    }
    usable.current = now;
  }, [tools, queryClient]);
}

function usableKey(tools: Tools): string {
  return tools.tools.map((tool) => `${tool.name}:${tool.version ?? ""}`).join(",");
}

/** One tool, from {@link useTools}. */
export function useTool(name: ToolName): Tool | undefined {
  return useTools().data?.tools.find((tool) => tool.name === name);
}

/** Asks the core to install what is missing and look for updates now. */
export function useCheckTools() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => expectData(await api.POST("/tools/check")),
    onSuccess: (tools) => queryClient.setQueryData(["tools"], tools),
  });
}
