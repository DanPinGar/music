-- Comments on a video: Markdown anchored to a moment (end_ms NULL) or a stretch. Times in ms.
CREATE TABLE comments (
    id         TEXT PRIMARY KEY NOT NULL,
    video_id   TEXT NOT NULL REFERENCES videos (id),
    start_ms   INTEGER NOT NULL CHECK (start_ms >= 0),
    end_ms     INTEGER CHECK (end_ms IS NULL OR end_ms > start_ms),
    body       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
) STRICT;

CREATE INDEX comments_by_video ON comments (video_id, start_ms) WHERE deleted_at IS NULL;
