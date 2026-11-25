import "./index.css";
import "./i18n";

import { createApiClient } from "@segno/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Toaster } from "sonner";
import { CoreProvider } from "./api/api-context";
import { router } from "./app/router";
import { CommandRegistryProvider } from "./commands/commands-context";
import { CommandRegistry } from "./commands/registry";
import { TooltipProvider } from "./components/ui/tooltip";
import { detectPlatform } from "./platform/platform";

async function bootstrap(root: HTMLElement) {
  const platform = detectPlatform();
  const config = await platform.apiConfig();
  const core = { client: createApiClient(config), config, platform };
  const queryClient = new QueryClient({
    defaultOptions: { queries: { staleTime: 30_000, retry: 1 } },
  });
  const commands = new CommandRegistry();

  createRoot(root).render(
    <StrictMode>
      <CoreProvider core={core}>
        <QueryClientProvider client={queryClient}>
          <CommandRegistryProvider registry={commands}>
            <TooltipProvider>
              <RouterProvider router={router} />
              <Toaster theme="dark" position="bottom-right" richColors />
            </TooltipProvider>
          </CommandRegistryProvider>
        </QueryClientProvider>
      </CoreProvider>
    </StrictMode>,
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element in index.html");
bootstrap(root).catch((error: unknown) => {
  console.error(error);
  root.textContent = `Segno could not start: ${error instanceof Error ? error.message : String(error)}`;
});
