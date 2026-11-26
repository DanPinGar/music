import { type Attacks, expectData, type Video } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { useApi } from "@/api/api-context";
import { buildWaveform, type Waveform } from "./timeline/waveform";

// Own query keys: invalidating ["videos"] after an import must not refetch these files.

/** The video's waveform, once the core has made it. */
export function useWaveform(video: Video | undefined): Waveform | null {
  const api = useApi();
  const id = video?.id ?? "";
  const query = useQuery({
    queryKey: ["peaks", id],
    queryFn: async () =>
      buildWaveform(expectData(await api.GET("/videos/{id}/peaks", { params: { path: { id } } }))),
    enabled: video?.has_peaks === true,
    staleTime: Number.POSITIVE_INFINITY,
  });
  return query.data ?? null;
}

/** The attacks (note onsets) found in the video's audio, once the core has found them. */
export function useAttacks(video: Video | undefined): Attacks | null {
  const api = useApi();
  const id = video?.id ?? "";
  const query = useQuery({
    queryKey: ["attacks", id],
    queryFn: async () =>
      expectData(await api.GET("/videos/{id}/attacks", { params: { path: { id } } })),
    enabled: video?.has_attacks === true,
    staleTime: Number.POSITIVE_INFINITY,
  });
  return query.data ?? null;
}
