/// <reference types="vitest/config" />
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Set by the Tauri CLI when the app runs on a device other than this computer.
const devHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "src") },
  },
  // Keep Rust compiler output visible when Tauri runs Vite.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: devHost || false,
    hmr: devHost ? { protocol: "ws", host: devHost, port: 5174 } : undefined,
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    // WebView2 (Chromium) on Windows, WebKit on macOS and Linux.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome110" : "safari15",
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
    // The desktop app loads its bundle from disk, so chunk size barely affects start-up.
    chunkSizeWarningLimit: 1500,
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
