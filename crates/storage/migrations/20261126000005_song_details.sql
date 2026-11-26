-- The song each video plays: the title is the song's; who wrote or plays it, and the year it
-- (or its album) was published, which the library is sorted by.
ALTER TABLE videos ADD COLUMN artist TEXT;
ALTER TABLE videos ADD COLUMN year INTEGER;

-- One-off work for the core, which removes the row when done: guess the artist and year of
-- the videos already in the library from their titles.
INSERT INTO settings (key, value, updated_at)
VALUES ('pending:guess_songs', '1', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
