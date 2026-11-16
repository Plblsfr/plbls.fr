CREATE EXTENSION IF NOT EXISTS unaccent;

CREATE FUNCTION sans_accents(texte text) RETURNS text
    LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT
    RETURN lower(public.unaccent('public.unaccent'::regdictionary, texte));

CREATE TEXT SEARCH CONFIGURATION sillon (COPY = simple);
ALTER TEXT SEARCH CONFIGURATION sillon
    ALTER MAPPING FOR asciiword, asciihword, hword_asciipart, word, hword, hword_part WITH public.unaccent, simple;

CREATE TABLE artistes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    mbid text NOT NULL UNIQUE,
    lidarr_id bigint,
    nom text NOT NULL,
    nom_tri text NOT NULL,
    resume text,
    image text,
    fond text,
    present boolean NOT NULL DEFAULT false,
    present_le timestamptz,
    cree_le timestamptz NOT NULL DEFAULT now(),
    tri text GENERATED ALWAYS AS (sans_accents(nom_tri)) STORED,
    cle text GENERATED ALWAYS AS (sans_accents(nom)) STORED,
    recherche tsvector GENERATED ALWAYS AS (to_tsvector('sillon'::regconfig, nom)) STORED
);
CREATE INDEX artistes_tri ON artistes (tri, id) WHERE present;
CREATE INDEX artistes_recherche ON artistes USING gin (recherche);

CREATE TABLE albums (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    mbid text NOT NULL UNIQUE,
    lidarr_id bigint,
    artiste_id uuid NOT NULL REFERENCES artistes (id),
    titre text NOT NULL,
    type text,
    types_secondaires text[] NOT NULL DEFAULT '{}',
    sortie date,
    annee integer,
    pochette text,
    present boolean NOT NULL DEFAULT false,
    present_le timestamptz,
    cree_le timestamptz NOT NULL DEFAULT now(),
    cle text GENERATED ALWAYS AS (sans_accents(titre)) STORED,
    recherche tsvector GENERATED ALWAYS AS (to_tsvector('sillon'::regconfig, titre)) STORED
);
CREATE INDEX albums_artiste ON albums (artiste_id);
CREATE INDEX albums_recents ON albums (present_le DESC, id DESC) WHERE present;
CREATE INDEX albums_recherche ON albums USING gin (recherche);

CREATE TABLE pistes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    album_id uuid NOT NULL REFERENCES albums (id),
    artiste_id uuid NOT NULL REFERENCES artistes (id),
    mbid_enregistrement text NOT NULL,
    mbid_piste text,
    lidarr_id bigint,
    titre text NOT NULL,
    disque integer NOT NULL DEFAULT 1,
    numero integer,
    duree double precision,
    explicite boolean NOT NULL DEFAULT false,
    present boolean NOT NULL DEFAULT false,
    present_le timestamptz,
    cree_le timestamptz NOT NULL DEFAULT now(),
    cle text GENERATED ALWAYS AS (sans_accents(titre)) STORED,
    recherche tsvector GENERATED ALWAYS AS (to_tsvector('sillon'::regconfig, titre)) STORED,
    UNIQUE (album_id, mbid_enregistrement)
);
CREATE INDEX pistes_album ON pistes (album_id, disque, numero);
CREATE INDEX pistes_artiste ON pistes (artiste_id);
CREATE INDEX pistes_enregistrement ON pistes (mbid_enregistrement);
CREATE INDEX pistes_recherche ON pistes USING gin (recherche);

CREATE TABLE fichiers (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    piste_id uuid NOT NULL REFERENCES pistes (id) ON DELETE CASCADE,
    lidarr_id bigint,
    chemin text NOT NULL UNIQUE,
    taille bigint NOT NULL,
    modifie_le timestamptz,
    contenu_le timestamptz NOT NULL DEFAULT now(),
    vu_le timestamptz NOT NULL DEFAULT now(),
    conteneur text NOT NULL,
    codec text NOT NULL,
    debit integer,
    frequence integer,
    canaux integer,
    bits integer,
    duree double precision
);
CREATE INDEX fichiers_piste ON fichiers (piste_id);
CREATE INDEX fichiers_contenu ON fichiers (contenu_le);

CREATE TABLE fichiers_ecartes (
    chemin text PRIMARY KEY,
    nom text NOT NULL,
    raison text NOT NULL,
    vu_le timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE genres (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    nom text NOT NULL UNIQUE
);

CREATE TABLE artistes_genres (
    artiste_id uuid NOT NULL REFERENCES artistes (id) ON DELETE CASCADE,
    genre_id bigint NOT NULL REFERENCES genres (id) ON DELETE CASCADE,
    PRIMARY KEY (artiste_id, genre_id)
);
CREATE INDEX artistes_genres_genre ON artistes_genres (genre_id);

CREATE TABLE albums_genres (
    album_id uuid NOT NULL REFERENCES albums (id) ON DELETE CASCADE,
    genre_id bigint NOT NULL REFERENCES genres (id) ON DELETE CASCADE,
    PRIMARY KEY (album_id, genre_id)
);
CREATE INDEX albums_genres_genre ON albums_genres (genre_id);

CREATE TABLE synchronisations (
    source text PRIMARY KEY,
    reussie_le timestamptz,
    essai_le timestamptz,
    erreur text
);

CREATE TABLE images (
    cle text PRIMARY KEY,
    version text NOT NULL,
    mime text NOT NULL,
    octets bytea NOT NULL,
    cree_le timestamptz NOT NULL DEFAULT now()
);
