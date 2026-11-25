-- Playable media of each video, relative to the library folder (e.g. media/<id>/original.mp4).
ALTER TABLE videos ADD COLUMN media_file TEXT;
