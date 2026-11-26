CREATE TABLE recurrents (
    sub text NOT NULL,
    commercant_id bigint NOT NULL REFERENCES commercants (id) ON DELETE CASCADE,
    cle text NOT NULL,
    ecarte boolean NOT NULL DEFAULT false,
    nouveau_vu boolean NOT NULL DEFAULT false,
    montant_vu bigint,
    modifie_le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, commercant_id, cle)
);
