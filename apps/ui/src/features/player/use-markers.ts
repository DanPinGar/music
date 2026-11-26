import { expectData, type Marker, type MarkerChanges, type NewMarker } from "@segno/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi } from "@/api/api-context";
import { type MarkerItem, toMarkerItems } from "./markers";

export interface MarkerActions {
  readonly items: readonly MarkerItem[];
  add(marker: NewMarker): void;
  change(id: string, changes: MarkerChanges): void;
  remove(id: string): void;
}

/**
 * The video's markers, with changes shown at once (optimistically) and rolled back with a
 * message if the core refuses them.
 */
export function useMarkers(videoId: string): MarkerActions {
  const { t } = useTranslation();
  const api = useApi();
  const queryClient = useQueryClient();
  const key = ["markers", videoId];
  const path = { params: { path: { id: videoId } } };

  const query = useQuery({
    queryKey: key,
    queryFn: async () => expectData(await api.GET("/videos/{id}/markers", path)),
  });

  /** Applies `update` to the cached list now; returns how to undo it. */
  const optimistic = async (update: (markers: Marker[]) => Marker[]) => {
    await queryClient.cancelQueries({ queryKey: key });
    const before = queryClient.getQueryData<Marker[]>(key) ?? [];
    queryClient.setQueryData<Marker[]>(key, update(before));
    return { before };
  };
  const settle = {
    onError: (error: Error, _: unknown, context?: { before: Marker[] }) => {
      if (context) queryClient.setQueryData(key, context.before);
      toast.error(t("markers.saveFailed"), { description: error.message });
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: key }),
  };

  const add = useMutation({
    mutationFn: async (marker: NewMarker) =>
      expectData(await api.POST("/videos/{id}/markers", { ...path, body: marker })),
    onMutate: (marker: NewMarker) =>
      optimistic((markers) => [
        ...markers,
        {
          id: `pending-${Date.now()}`,
          kind: marker.kind,
          start_ms: marker.start_ms,
          end_ms: marker.end_ms ?? null,
          label: marker.label ?? "",
          color: marker.color ?? null,
          created_at: "",
          updated_at: "",
        },
      ]),
    ...settle,
  });

  const change = useMutation({
    mutationFn: async ({ id, changes }: { id: string; changes: MarkerChanges }) =>
      expectData(
        await api.PUT("/videos/{id}/markers/{marker_id}", {
          params: { path: { id: videoId, marker_id: id } },
          body: changes,
        }),
      ),
    onMutate: ({ id, changes }) =>
      optimistic((markers) =>
        markers.map((marker) =>
          marker.id === id
            ? {
                ...marker,
                start_ms: changes.start_ms,
                end_ms: changes.end_ms ?? null,
                label: changes.label ?? "",
                color: changes.color ?? null,
              }
            : marker,
        ),
      ),
    ...settle,
  });

  const remove = useMutation({
    mutationFn: async (id: string) => {
      const result = await api.DELETE("/videos/{id}/markers/{marker_id}", {
        params: { path: { id: videoId, marker_id: id } },
      });
      if (result.error !== undefined) expectData(result);
    },
    onMutate: (id: string) => optimistic((markers) => markers.filter((marker) => marker.id !== id)),
    ...settle,
  });

  const items = useMemo(() => toMarkerItems(query.data ?? []), [query.data]);
  return {
    items,
    add: (marker) => add.mutate(marker),
    change: (id, changes) => change.mutate({ id, changes }),
    remove: (id) => remove.mutate(id),
  };
}
