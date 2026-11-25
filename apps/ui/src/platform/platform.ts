import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

export interface ApiConfig {
  readonly baseUrl: string;
  readonly token: string;
}

/** Callbacks for files dragged over the window from the operating system. */
export interface FileDropHandlers {
  over(): void;
  leave(): void;
  drop(paths: string[]): void;
}

/** Video formats offered in the file picker and accepted when dropped. */
export const VIDEO_EXTENSIONS = ["mp4", "m4v", "mkv", "webm", "mov", "avi"] as const;

export function isVideoPath(path: string): boolean {
  const extension = path.split(".").pop()?.toLowerCase() ?? "";
  return (VIDEO_EXTENSIONS as readonly string[]).includes(extension);
}

/**
 * What the host provides: the desktop app (Tauri) or a plain browser.
 *
 * Everything native goes through this interface so the same UI runs unchanged on the
 * desktop and, later, on the web.
 */
export interface Platform {
  readonly kind: "desktop" | "web";
  /** Where the core API lives and how to authenticate with it. */
  apiConfig(): Promise<ApiConfig>;
  /** Lets the user pick a video file; its absolute path, or null when cancelled. */
  pickVideoFile(title: string): Promise<string | null>;
  /** Watches files dragged onto the window; returns a function that stops watching. */
  watchFileDrops(handlers: FileDropHandlers): Promise<() => void>;
}

const desktop: Platform = {
  kind: "desktop",
  async apiConfig() {
    const config = await invoke<{ base_url: string; token: string }>("api_config");
    return { baseUrl: config.base_url, token: config.token };
  },
  async pickVideoFile(title) {
    const path = await open({
      title,
      multiple: false,
      directory: false,
      filters: [{ name: "Video", extensions: [...VIDEO_EXTENSIONS] }],
    });
    return typeof path === "string" ? path : null;
  },
  async watchFileDrops(handlers) {
    return getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") handlers.over();
      else if (payload.type === "leave") handlers.leave();
      else handlers.drop(payload.paths);
    });
  },
};

/** Browser development against `segno-api serve` (see the root README). */
const web: Platform = {
  kind: "web",
  async apiConfig() {
    return {
      baseUrl: import.meta.env.VITE_API_URL ?? "http://127.0.0.1:7700",
      token: import.meta.env.VITE_API_TOKEN ?? "dev",
    };
  },
  async pickVideoFile(title) {
    // Browsers do not reveal file paths; the API dev server reads files by path.
    return window.prompt(title)?.trim() || null;
  },
  async watchFileDrops() {
    return () => {};
  },
};

export function detectPlatform(): Platform {
  return isTauri() ? desktop : web;
}
