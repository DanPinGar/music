/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Core API address for browser development (default http://127.0.0.1:7700). */
  readonly VITE_API_URL?: string;
  /** Token for `segno-api serve` (default "dev"). */
  readonly VITE_API_TOKEN?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
