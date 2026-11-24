-- Videos in the library. Ids are UUIDv7 strings; timestamps are UTC (RFC 3339 text).
-- Every row keeps created_at / updated_at and uses soft deletes, ready for future sync.
CREATE TABLE videos (
    id          TEXT PRIMARY KEY NOT NULL,
    source      TEXT NOT NULL CHECK (source IN ('youtube', 'file')),
    source_url  TEXT,
    title       TEXT NOT NULL,
    channel     TEXT,
    duration_ms INTEGER,
    fps         REAL,
    width       INTEGER,
    height      INTEGER,
    status      TEXT NOT NULL CHECK (status IN ('importing', 'ready', 'failed')),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT
) STRICT;

CREATE INDEX videos_by_created_at ON videos (created_at) WHERE deleted_at IS NULL;
