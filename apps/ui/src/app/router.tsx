import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { LibraryPage } from "@/features/library/library-page";
import { PlayerPage } from "@/features/player/player-page";
import { AppShell } from "./app-shell";

const rootRoute = createRootRoute({ component: AppShell });

const libraryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: LibraryPage,
});

const playerRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/player",
  component: PlayerPage,
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([libraryRoute, playerRoute]),
  // Hash URLs survive reloads inside the webview, which serves a single index.html.
  history: createHashHistory(),
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
