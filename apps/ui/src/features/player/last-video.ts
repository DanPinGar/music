// One entry per library identity (`library_id` from /system): every library shares the
// webview's storage, including throwaway test libraries (see SEGNO_LIBRARY_DIR) that are
// recreated in the same folder.
const key = (library: string) => `segno.lastVideo:${library}`;

/** The video studied most recently in `library`, reopened by the "Player" tab. */
export function lastVideoId(library: string): string | null {
  try {
    return localStorage.getItem(key(library));
  } catch {
    return null;
  }
}

export function rememberLastVideo(library: string, videoId: string): void {
  try {
    localStorage.setItem(key(library), videoId);
  } catch {
    // Storage unavailable: the Player tab will simply not reopen it.
  }
}

export function forgetLastVideo(library: string): void {
  try {
    localStorage.removeItem(key(library));
  } catch {
    // Nothing to forget.
  }
}
