-- Salon « musique » : un salon vocal que le bot Estompe rejoint ; un seul par serveur.
ALTER TABLE channels DROP CONSTRAINT channels_kind_check;
ALTER TABLE channels ADD CONSTRAINT channels_kind_check CHECK (kind IN ('text', 'voice', 'music'));
CREATE UNIQUE INDEX channels_one_music ON channels (server_id) WHERE kind = 'music';
