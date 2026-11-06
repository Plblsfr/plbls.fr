-- Salon « Musique » (EST-171) : même modèle que le salon « Vidéos » (cartes, listes, séances planifiées), réservé aux liens YouTube Music.
ALTER TABLE channels DROP CONSTRAINT channels_kind_check;
ALTER TABLE channels ADD CONSTRAINT channels_kind_check CHECK (kind IN ('text', 'voice', 'videos', 'music'));

-- Liste collaborative (tout membre qui écrit peut la modifier) ou réservée à son créateur.
ALTER TABLE video_lists ADD COLUMN collaborative boolean NOT NULL DEFAULT true;
