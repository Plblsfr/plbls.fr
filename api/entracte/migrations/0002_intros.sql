CREATE TABLE reglages (
    cle text PRIMARY KEY,
    valeur text NOT NULL
);

CREATE TABLE empreintes (
    fichier_id uuid PRIMARY KEY REFERENCES fichiers(id) ON DELETE CASCADE,
    taille bigint NOT NULL,
    modifie_le timestamptz,
    points bytea NOT NULL,
    cree_le timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE intros_saisons (
    serie_id uuid NOT NULL REFERENCES titres(id) ON DELETE CASCADE,
    saison integer NOT NULL,
    etat text NOT NULL CHECK (etat IN ('en_file', 'en_cours', 'trouvee', 'rien', 'erreur')),
    signature text NOT NULL DEFAULT '',
    erreur text,
    demande_le timestamptz NOT NULL DEFAULT now(),
    maj timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (serie_id, saison)
);

CREATE INDEX intros_saisons_file ON intros_saisons (demande_le) WHERE etat = 'en_file';

CREATE TABLE intros (
    titre_id uuid PRIMARY KEY REFERENCES titres(id) ON DELETE CASCADE,
    fichier_id uuid NOT NULL REFERENCES fichiers(id) ON DELETE CASCADE,
    debut double precision NOT NULL,
    fin double precision NOT NULL,
    CHECK (debut >= 0 AND fin > debut)
);

CREATE INDEX intros_fichier ON intros (fichier_id);
