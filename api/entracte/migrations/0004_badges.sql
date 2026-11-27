ALTER TABLE progression ADD COLUMN secondes_vues double precision NOT NULL DEFAULT 0,
    ADD COLUMN intro_creditee boolean NOT NULL DEFAULT false,
    ADD COLUMN fin_creditee boolean NOT NULL DEFAULT false;

CREATE TABLE regardes (
    sub text NOT NULL,
    titre_id uuid NOT NULL REFERENCES titres(id) ON DELETE CASCADE,
    regarde_le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, titre_id)
);

CREATE TABLE badges (
    sub text NOT NULL,
    genre text NOT NULL CHECK (genre IN ('film', 'saison', 'serie')),
    titre_id uuid NOT NULL REFERENCES titres(id) ON DELETE CASCADE,
    saison integer,
    obtenu_le timestamptz NOT NULL,
    CHECK ((genre = 'saison') = (saison IS NOT NULL))
);

CREATE UNIQUE INDEX badges_uniques ON badges (sub, genre, titre_id, coalesce(saison, -1));
CREATE INDEX badges_personne ON badges (sub, obtenu_le DESC);

INSERT INTO regardes (sub, titre_id, regarde_le) SELECT sub, titre_id, vu_le FROM vus;
