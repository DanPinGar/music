import { ApiRequestError, expectData, type Problem, type Video } from "@segno/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi } from "@/api/api-context";
import { useSystemInfo } from "@/api/use-system-info";
import { forgetLastVideo, lastVideoId } from "@/features/player/last-video";

export interface VideoChanges {
  readonly title?: string;
  readonly tags?: string[];
}

/** What the library can do to its videos, with the messages that tell how it went. */
export function useLibraryActions() {
  const { t } = useTranslation();
  const api = useApi();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const library = useSystemInfo().data?.library_id;
  const refresh = useCallback(
    () => queryClient.invalidateQueries({ queryKey: ["videos"] }),
    [queryClient],
  );
  const fail = useCallback(
    (title: string, error: unknown) =>
      toast.error(title, { description: error instanceof Error ? error.message : String(error) }),
    [],
  );

  return useMemo(
    () => ({
      /** Queues a YouTube video and shows the library, where its progress appears. */
      download: async (url: string): Promise<Video | null> => {
        try {
          const result = await api.POST("/videos/download", { body: { url } });
          const video = expectData(result);
          if (result.response.status === 200) {
            toast(t("library.downloadExists", { title: video.title }));
          } else {
            toast.success(t("library.downloadQueued"));
          }
          await refresh();
          void navigate({ to: "/" });
          return video;
        } catch (error) {
          fail(t("library.downloadFailed"), error);
          return null;
        }
      },
      retry: async (video: Video) => {
        try {
          expectData(await api.POST("/videos/{id}/retry", { params: { path: { id: video.id } } }));
          await refresh();
        } catch (error) {
          fail(t("library.downloadFailed"), error);
        }
      },
      /** Deletes a video (stopping its download), and forgets it as the Player tab's video. */
      remove: async (video: Video) => {
        const result = await api.DELETE("/videos/{id}", { params: { path: { id: video.id } } });
        if (!result.response.ok) {
          const problem = result.error as Problem | undefined;
          fail(t("library.deleteFailed"), new ApiRequestError(result.response.status, problem));
          return;
        }
        if (library && lastVideoId(library) === video.id) forgetLastVideo(library);
        toast(t("library.deleted", { title: video.title }));
        await refresh();
      },
      edit: async (video: Video, changes: VideoChanges) => {
        try {
          expectData(
            await api.PATCH("/videos/{id}", {
              params: { path: { id: video.id } },
              body: changes,
            }),
          );
          await refresh();
        } catch (error) {
          fail(t("library.editFailed"), error);
        }
      },
    }),
    [api, t, refresh, navigate, fail, library],
  );
}
