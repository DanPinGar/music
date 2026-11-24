import type { ApiClient } from "@segno/api-client";
import { createContext, type ReactNode, useContext } from "react";

const ApiContext = createContext<ApiClient | null>(null);

export function ApiProvider({ client, children }: { client: ApiClient; children: ReactNode }) {
  return <ApiContext value={client}>{children}</ApiContext>;
}

/** The typed client of the core API. */
export function useApi(): ApiClient {
  const client = useContext(ApiContext);
  if (!client) throw new Error("useApi must be used inside <ApiProvider>");
  return client;
}
