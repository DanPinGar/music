-- Library-wide settings and facts as key/value pairs (values are plain text or JSON).
CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

-- Every library has its own identity, so state kept outside the database (in the UI, in a
-- future cloud account) never mixes two libraries, not even when a new library replaces an
-- old one in the same folder.
INSERT INTO settings (key, value, updated_at)
VALUES ('library_id', lower(hex(randomblob(16))), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
