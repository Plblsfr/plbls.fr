CREATE TABLE ajouts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    type text NOT NULL CHECK (type IN ('album', 'artiste')),
    mbid text NOT NULL,
    titre text NOT NULL,
    artiste_mbid text NOT NULL,
    artiste_nom text NOT NULL,
    origine text NOT NULL DEFAULT 'manuel' CHECK (origine IN ('manuel', 'decouverte')),
    sub text NOT NULL,
    nom text NOT NULL,
    etat text NOT NULL DEFAULT 'envoye'
        CHECK (etat IN ('envoye', 'telechargement', 'partiel', 'disponible', 'echec', 'supprime')),
    raison text,
    estimation bigint NOT NULL CHECK (estimation >= 0),
    albums text[] NOT NULL DEFAULT '{}',
    lidarr_artiste_id bigint,
    lidarr_albums bigint[] NOT NULL DEFAULT '{}',
    surveille boolean NOT NULL DEFAULT false,
    cree_le timestamptz NOT NULL DEFAULT now(),
    envoye_le timestamptz NOT NULL DEFAULT now(),
    maj timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ajouts_personne ON ajouts (sub, origine, cree_le DESC);
CREATE INDEX ajouts_etat ON ajouts (etat);
CREATE INDEX ajouts_albums ON ajouts USING gin (albums);
CREATE UNIQUE INDEX ajouts_actifs ON ajouts (type, mbid) WHERE etat IN ('envoye', 'telechargement', 'partiel');

CREATE TABLE exclusions (
    mbid text PRIMARY KEY,
    type text NOT NULL CHECK (type IN ('album', 'artiste')),
    titre text NOT NULL,
    par text NOT NULL,
    le timestamptz NOT NULL DEFAULT now()
);
