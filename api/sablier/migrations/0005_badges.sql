CREATE TABLE badges (
    sub text NOT NULL,
    genre text NOT NULL CHECK (genre IN ('film', 'saison', 'serie')),
    titre text NOT NULL,
    saison integer NOT NULL DEFAULT 0,
    obtenu_le timestamptz NOT NULL,
    partage boolean NOT NULL DEFAULT false,
    PRIMARY KEY (sub, genre, titre, saison),
    CHECK ((genre = 'saison') = (saison > 0))
);
