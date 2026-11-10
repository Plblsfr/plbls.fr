CREATE TABLE titres (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    genre text NOT NULL CHECK (genre IN ('film', 'serie', 'episode')),
    tmdb_id bigint NOT NULL,
    serie_id uuid REFERENCES titres(id) ON DELETE CASCADE,
    saison integer,
    episode integer,
    titre text NOT NULL,
    titre_original text,
    annee integer,
    resume text,
    genres text[] NOT NULL DEFAULT '{}',
    affiche text,
    fond text,
    duree double precision,
    present boolean NOT NULL DEFAULT false,
    radarr_id bigint,
    sonarr_id bigint,
    metadonnees_le timestamptz,
    ajoute_le timestamptz NOT NULL DEFAULT now(),
    present_le timestamptz,
    CHECK ((genre = 'episode') = (serie_id IS NOT NULL AND saison IS NOT NULL AND episode IS NOT NULL))
);

CREATE UNIQUE INDEX titres_tmdb ON titres (genre, tmdb_id) WHERE genre <> 'episode';
CREATE UNIQUE INDEX titres_episode ON titres (serie_id, saison, episode) WHERE genre = 'episode';
CREATE INDEX titres_present ON titres (genre, present);

CREATE TABLE saisons (
    serie_id uuid NOT NULL REFERENCES titres(id) ON DELETE CASCADE,
    numero integer NOT NULL,
    titre text NOT NULL,
    episodes integer NOT NULL DEFAULT 0,
    affiche text,
    PRIMARY KEY (serie_id, numero)
);

CREATE TABLE fichiers (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    titre_id uuid NOT NULL REFERENCES titres(id) ON DELETE CASCADE,
    chemin text NOT NULL UNIQUE,
    taille bigint NOT NULL,
    modifie_le timestamptz,
    infos jsonb NOT NULL,
    vu_le timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX fichiers_titre ON fichiers (titre_id);

CREATE TABLE fichiers_ecartes (
    chemin text PRIMARY KEY,
    nom text NOT NULL,
    raison text NOT NULL,
    vu_le timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE images (
    cle text PRIMARY KEY,
    mime text NOT NULL,
    octets bytea NOT NULL,
    maj timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sous_titres (
    fichier_id uuid NOT NULL REFERENCES fichiers(id) ON DELETE CASCADE,
    piste text NOT NULL,
    vtt text NOT NULL,
    PRIMARY KEY (fichier_id, piste)
);

CREATE TABLE synchronisations (
    source text PRIMARY KEY,
    reussie_le timestamptz,
    essai_le timestamptz,
    erreur text
);

CREATE TABLE progression (
    sub text NOT NULL,
    titre_id uuid NOT NULL REFERENCES titres(id) ON DELETE CASCADE,
    position double precision NOT NULL,
    duree double precision NOT NULL,
    termine boolean NOT NULL DEFAULT false,
    maj timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, titre_id)
);

CREATE INDEX progression_reprise ON progression (sub, maj DESC) WHERE NOT termine;

CREATE TABLE vus (
    sub text NOT NULL,
    titre_id uuid NOT NULL REFERENCES titres(id) ON DELETE CASCADE,
    vu_le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, titre_id)
);

CREATE INDEX vus_historique ON vus (sub, vu_le DESC);

CREATE TABLE demandes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    genre text NOT NULL CHECK (genre IN ('film', 'serie')),
    tmdb_id bigint NOT NULL,
    saisons integer[],
    titre text NOT NULL,
    annee integer,
    affiche text,
    etat text NOT NULL DEFAULT 'en_attente' CHECK (etat IN ('en_attente', 'acceptee', 'refusee', 'disponible')),
    motif text,
    envoyee boolean NOT NULL DEFAULT false,
    erreur_envoi text,
    traitee_par text,
    cree_le timestamptz NOT NULL DEFAULT now(),
    maj timestamptz NOT NULL DEFAULT now(),
    CHECK (etat <> 'refusee' OR length(coalesce(motif, '')) > 0)
);

CREATE UNIQUE INDEX demandes_en_cours ON demandes (genre, tmdb_id, coalesce(saisons, '{}'))
    WHERE etat IN ('en_attente', 'acceptee');
CREATE INDEX demandes_etat ON demandes (etat, cree_le DESC);

CREATE TABLE demandeurs (
    demande_id uuid NOT NULL REFERENCES demandes(id) ON DELETE CASCADE,
    sub text NOT NULL,
    nom text NOT NULL DEFAULT '',
    rattache_le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (demande_id, sub)
);

CREATE INDEX demandeurs_sub ON demandeurs (sub, rattache_le DESC);

CREATE TABLE abonnements_push (
    endpoint text PRIMARY KEY,
    sub text NOT NULL,
    p256dh text NOT NULL DEFAULT '',
    auth text NOT NULL DEFAULT '',
    genre text NOT NULL CHECK (genre IN ('web', 'fcm')),
    cree_le timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX abonnements_push_sub ON abonnements_push (sub);

CREATE TABLE notifications (
    id bigserial PRIMARY KEY,
    sub text NOT NULL,
    genre text NOT NULL,
    demande_id uuid REFERENCES demandes(id) ON DELETE CASCADE,
    titre text NOT NULL,
    corps text NOT NULL,
    cree_le timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX notifications_sub ON notifications (sub, id DESC);
