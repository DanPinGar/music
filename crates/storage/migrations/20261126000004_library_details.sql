-- Why a download or import failed, if it did.
ALTER TABLE videos ADD COLUMN import_error TEXT;

-- The user's tags for organising the library, in the order they were given.
CREATE TABLE video_tags (
    video_id TEXT NOT NULL REFERENCES videos (id),
    tag      TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (video_id, tag)
) STRICT;
