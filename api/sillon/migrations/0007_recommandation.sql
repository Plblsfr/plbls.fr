ALTER TABLE preferences ADD COLUMN decouverte boolean NOT NULL DEFAULT true;

CREATE TABLE similarites_releves (
    genre text NOT NULL CHECK (genre IN ('similaires', 'populaires')),
    mbid text NOT NULL,
    releve_le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (genre, mbid)
);

CREATE TABLE artistes_similaires (
    mbid text NOT NULL,
    similaire text NOT NULL,
    nom text NOT NULL,
    score real NOT NULL CHECK (score >= 0 AND score <= 1),
    PRIMARY KEY (mbid, similaire)
);
CREATE INDEX artistes_similaires_inverse ON artistes_similaires (similaire);

CREATE TABLE enregistrements_populaires (
    artiste text NOT NULL,
    rang integer NOT NULL,
    mbid text NOT NULL,
    titre text NOT NULL,
    album_mbid text,
    album_titre text,
    ecoutes bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (artiste, rang)
);

CREATE TABLE refus (
    sub text NOT NULL,
    piste_id uuid NOT NULL REFERENCES pistes (id) ON DELETE CASCADE,
    le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, piste_id)
);

CREATE TABLE refus_albums (
    sub text NOT NULL,
    mbid text NOT NULL,
    le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, mbid)
);

ALTER TABLE ajouts
    ADD COLUMN similaire_a text,
    ADD COLUMN enregistrement text,
    ADD COLUMN titre_piste text,
    ADD COLUMN promu_le timestamptz;
