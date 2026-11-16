CREATE TABLE analyses (
    piste_id uuid PRIMARY KEY REFERENCES pistes (id) ON DELETE CASCADE,
    contenu_le timestamptz NOT NULL,
    version integer NOT NULL,
    vecteur real[] NOT NULL,
    tempo real,
    tonalite smallint NOT NULL,
    majeur boolean NOT NULL,
    analyse_le timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE taches_analyse (
    fichier_id uuid PRIMARY KEY REFERENCES fichiers (id) ON DELETE CASCADE,
    contenu_le timestamptz NOT NULL,
    version integer NOT NULL,
    etat text NOT NULL DEFAULT 'attente' CHECK (etat IN ('attente', 'en_cours', 'faite', 'erreur')),
    tentatives integer NOT NULL DEFAULT 0,
    erreur text,
    modifie_le timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX taches_analyse_attente ON taches_analyse (tentatives, contenu_le DESC, fichier_id) WHERE etat = 'attente';
