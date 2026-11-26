-- Markers on a video's timeline: points, named sections and saved loops. Times in ms.
CREATE TABLE markers (
    id         TEXT PRIMARY KEY NOT NULL,
    video_id   TEXT NOT NULL REFERENCES videos (id),
    kind       TEXT NOT NULL CHECK (kind IN ('point', 'section', 'loop')),
    start_ms   INTEGER NOT NULL CHECK (start_ms >= 0),
    end_ms     INTEGER CHECK (end_ms IS NULL OR end_ms > start_ms),
    label      TEXT NOT NULL,
    color      TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
) STRICT;

CREATE INDEX markers_by_video ON markers (video_id, start_ms) WHERE deleted_at IS NULL;

-- Where the user left each video (position, speed, zoom, beat grid, loop…), as a JSON document
-- whose fields all have defaults, so it can grow without migrations.
CREATE TABLE video_states (
    video_id   TEXT PRIMARY KEY NOT NULL REFERENCES videos (id),
    state      TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;
