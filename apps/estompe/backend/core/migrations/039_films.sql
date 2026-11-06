CREATE TABLE films_items (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    jellyfin_id text UNIQUE NOT NULL
);

CREATE TABLE films_progression (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id uuid NOT NULL REFERENCES films_items(id) ON DELETE CASCADE,
    position double precision NOT NULL,
    duree double precision NOT NULL,
    vu boolean NOT NULL DEFAULT false,
    maj timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, item_id)
);

CREATE INDEX films_progression_reprise ON films_progression (user_id, maj DESC) WHERE NOT vu;

CREATE TABLE films_rejets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id uuid NOT NULL REFERENCES films_items(id) ON DELETE CASCADE,
    nom text NOT NULL,
    raison text NOT NULL,
    vu_le timestamptz NOT NULL DEFAULT now(),
    UNIQUE (item_id, raison)
);
