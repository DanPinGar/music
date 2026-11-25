-- Files derived from each video's media to make navigation instant, relative to the library
-- folder, and why preparing them failed, if it did.
ALTER TABLE videos ADD COLUMN proxy_file TEXT;
ALTER TABLE videos ADD COLUMN thumbnails_file TEXT;
ALTER TABLE videos ADD COLUMN peaks_file TEXT;
ALTER TABLE videos ADD COLUMN preparation_error TEXT;
