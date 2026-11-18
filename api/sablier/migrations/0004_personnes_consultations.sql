CREATE TABLE personnes (
    sub text PRIMARY KEY,
    nom text NOT NULL,
    vu_le timestamptz NOT NULL
);

CREATE TABLE consultations (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    admin_sub text NOT NULL,
    admin_nom text NOT NULL,
    cible_sub text NOT NULL,
    action text NOT NULL CHECK (action IN ('consultation', 'effacement')),
    date timestamptz NOT NULL
);

CREATE INDEX consultations_cible ON consultations (cible_sub, id);
CREATE INDEX consultations_admin_cible ON consultations (admin_sub, cible_sub, date);
