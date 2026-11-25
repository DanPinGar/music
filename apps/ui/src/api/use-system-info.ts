import { expectData } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { useApi } from "./api-context";

/** Version, library folder and tools of the running core (cached, shared by every caller). */
export function useSystemInfo() {
  const api = useApi();
  return useQuery({
    queryKey: ["system"],
    queryFn: async () => expectData(await api.GET("/system")),
  });
}
