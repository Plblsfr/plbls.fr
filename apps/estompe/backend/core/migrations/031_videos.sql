-- Vidéos (replays) : pièce jointe de type « video » (durée lue côté serveur) avec une miniature JPEG de 64 Ko maximum.
ALTER TABLE files DROP CONSTRAINT files_kind_check;
ALTER TABLE files ADD CONSTRAINT files_kind_check CHECK (kind IN ('file', 'voice', 'video'));
ALTER TABLE files ADD COLUMN thumb bytea;
