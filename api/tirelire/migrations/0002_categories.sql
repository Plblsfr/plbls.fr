CREATE TABLE personnes (
    sub text PRIMARY KEY,
    normalisation integer NOT NULL DEFAULT 0,
    cree_le timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE categories (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sub text NOT NULL,
    cle text,
    nom text NOT NULL,
    nature text NOT NULL CHECK (nature IN ('fixe', 'variable', 'epargne', 'revenu', 'neutre')),
    plafond bigint CHECK (plafond IS NULL OR plafond >= 0),
    couleur integer NOT NULL DEFAULT 0 CHECK (couleur >= 0),
    rang integer NOT NULL DEFAULT 0,
    archivee boolean NOT NULL DEFAULT false,
    cree_le timestamptz NOT NULL DEFAULT now(),
    UNIQUE (sub, cle)
);

CREATE UNIQUE INDEX categories_sub_nom ON categories (sub, lower(nom));

CREATE TABLE commercants (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sub text NOT NULL,
    nom text NOT NULL,
    categorie_id bigint REFERENCES categories (id) ON DELETE SET NULL,
    cree_le timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX commercants_sub ON commercants (sub, id);

CREATE TABLE variantes (
    sub text NOT NULL,
    cle text NOT NULL,
    commercant_id bigint NOT NULL REFERENCES commercants (id) ON DELETE CASCADE,
    manuelle boolean NOT NULL DEFAULT false,
    PRIMARY KEY (sub, cle)
);

CREATE INDEX variantes_commercant ON variantes (commercant_id);

CREATE TABLE regles (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sub text NOT NULL,
    cle text,
    categorie_id bigint NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
    mots text[] NOT NULL DEFAULT '{}',
    type text,
    section text,
    rang integer NOT NULL,
    cree_le timestamptz NOT NULL DEFAULT now(),
    UNIQUE (sub, cle),
    CHECK (cardinality(mots) > 0 OR type IS NOT NULL OR section IS NOT NULL)
);

CREATE INDEX regles_sub_rang ON regles (sub, rang, id);

ALTER TABLE operations
    ADD COLUMN commercant_cle text,
    ADD COLUMN origine text CHECK (origine IN ('operation', 'commercant', 'regle')),
    ADD COLUMN regle_id bigint REFERENCES regles (id) ON DELETE SET NULL,
    ADD CONSTRAINT operations_commercant_fk FOREIGN KEY (commercant_id) REFERENCES commercants (id) ON DELETE SET NULL,
    ADD CONSTRAINT operations_categorie_fk FOREIGN KEY (categorie_id) REFERENCES categories (id) ON DELETE SET NULL;

CREATE INDEX operations_sub_cle ON operations (sub, commercant_cle) WHERE commercant_cle IS NOT NULL;
CREATE INDEX operations_a_classer ON operations (sub, date_operation) WHERE categorie_id IS NULL;
