import type { ApiClient } from "@segno/api-client";
import { createContext, type ReactNode, useContext } from "react";
import type { ApiConfig, Platform } from "@/platform/platform";

interface Core {
  readonly client: ApiClient;
  readonly config: ApiConfig;
  readonly platform: Platform;
}

const CoreContext = createContext<Core | null>(null);

export function CoreProvider({ core, children }: { core: Core; children: ReactNode }) {
  return <CoreContext value={core}>{children}</CoreContext>;
}

function useCore(): Core {
  const core = useContext(CoreContext);
  if (!core) throw new Error("the core hooks must be used inside <CoreProvider>");
  return core;
}

/** The typed client of the core API. */
export function useApi(): ApiClient {
  return useCore().client;
}

/** Address and token of the core API (e.g. to build media URLs). */
export function useApiConfig(): ApiConfig {
  return useCore().config;
}

/** Native capabilities of the host: desktop app or browser. */
export function usePlatform(): Platform {
  return useCore().platform;
}
