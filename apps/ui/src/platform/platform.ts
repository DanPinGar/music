import { invoke, isTauri } from "@tauri-apps/api/core";

export interface ApiConfig {
  readonly baseUrl: string;
  readonly token: string;
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
}

const desktop: Platform = {
  kind: "desktop",
  async apiConfig() {
    const config = await invoke<{ base_url: string; token: string }>("api_config");
    return { baseUrl: config.base_url, token: config.token };
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
};

export function detectPlatform(): Platform {
  return isTauri() ? desktop : web;
}
