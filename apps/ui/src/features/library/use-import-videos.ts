import { expectData } from "@segno/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi, usePlatform } from "@/api/api-context";
import { isVideoPath } from "@/platform/platform";

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/**
 * Importing videos into the library, from the file picker or from dropped files. A single
 * video opens in the player once imported.
 */
export function useImportVideos() {
  const { t } = useTranslation();
  const api = useApi();
  const platform = usePlatform();
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  const importFiles = useCallback(
    async (paths: string[]) => {
      const videos = paths.filter(isVideoPath);
      if (videos.length === 0) {
        if (paths.length > 0) toast.error(t("library.notVideos"));
        return;
      }
      let lastImported: string | undefined;
      for (const path of videos) {
        const name = fileName(path);
        const id = toast.loading(t("library.importing", { name }));
        try {
          const video = expectData(await api.POST("/videos/import", { body: { path } }));
          toast.success(t("library.imported", { title: video.title }), { id });
          lastImported = video.id;
        } catch (error) {
          const reason = error instanceof Error ? error.message : String(error);
          toast.error(t("library.importFailed", { name }), { id, description: reason });
        }
      }
      await queryClient.invalidateQueries({ queryKey: ["videos"] });
      if (videos.length === 1 && lastImported) {
        await navigate({ to: "/videos/$videoId", params: { videoId: lastImported } });
      }
    },
    [api, navigate, queryClient, t],
  );

  const pickAndImport = useCallback(async () => {
    const path = await platform.pickVideoFile(t("library.pickTitle"));
    if (path) await importFiles([path]);
  }, [importFiles, platform, t]);

  return { importFiles, pickAndImport };
}
