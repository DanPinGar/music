import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { LibraryPage } from "@/features/library/library-page";
import { PlayerPage } from "@/features/player/player-page";
import { VideoPlayerPage } from "@/features/player/video-player-page";
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

const videoRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/videos/$videoId",
  component: function VideoRoute() {
    const { videoId } = videoRoute.useParams();
    // A new key per video resets the player instead of reusing the previous video's state.
    return <VideoPlayerPage key={videoId} videoId={videoId} />;
  },
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([libraryRoute, playerRoute, videoRoute]),
  // Hash URLs survive reloads inside the webview, which serves a single index.html.
  history: createHashHistory(),
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
