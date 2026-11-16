CREATE TABLE mesures (
    piste_id uuid NOT NULL REFERENCES pistes (id) ON DELETE CASCADE,
    contenu_le timestamptz NOT NULL,
    niveau double precision NOT NULL,
    pic double precision NOT NULL,
    plage double precision NOT NULL,
    echantillons bigint NOT NULL,
    frequence integer NOT NULL,
    mesure_le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (piste_id, contenu_le)
);

CREATE TABLE preferences (
    sub text PRIMARY KEY,
    qualite text NOT NULL CHECK (qualite IN ('originale', 'haute', 'normale', 'basse')),
    modifie_le timestamptz NOT NULL DEFAULT now()
);
