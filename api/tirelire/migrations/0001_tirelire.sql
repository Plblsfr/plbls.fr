CREATE TABLE comptes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sub text NOT NULL,
    banque text NOT NULL,
    iban text NOT NULL,
    bic text NOT NULL DEFAULT '',
    numero text NOT NULL DEFAULT '',
    reference text NOT NULL DEFAULT '',
    titulaire text NOT NULL DEFAULT '',
    cree_le timestamptz NOT NULL DEFAULT now(),
    UNIQUE (sub, banque, iban)
);

CREATE TABLE releves (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    compte_id bigint NOT NULL REFERENCES comptes (id) ON DELETE CASCADE,
    sub text NOT NULL,
    numero integer NOT NULL,
    annee integer NOT NULL,
    debut date NOT NULL,
    date_arrete date NOT NULL,
    ancien_solde bigint NOT NULL,
    nouveau_solde bigint NOT NULL,
    total_debits bigint NOT NULL,
    total_credits bigint NOT NULL,
    pages integer NOT NULL,
    empreinte text NOT NULL,
    controle jsonb NOT NULL,
    importe_le timestamptz NOT NULL DEFAULT now(),
    UNIQUE (compte_id, annee, numero, date_arrete)
);

CREATE INDEX releves_sub_empreinte ON releves (sub, empreinte);
CREATE INDEX releves_sub_date ON releves (sub, date_arrete);

CREATE TABLE operations (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    releve_id bigint NOT NULL REFERENCES releves (id) ON DELETE CASCADE,
    compte_id bigint NOT NULL REFERENCES comptes (id) ON DELETE CASCADE,
    sub text NOT NULL,
    rang integer NOT NULL,
    page integer NOT NULL,
    date_operation date NOT NULL,
    date_valeur date NOT NULL,
    date_achat date,
    section text NOT NULL,
    type text NOT NULL,
    libelle text NOT NULL,
    details text[] NOT NULL DEFAULT '{}',
    commercant_brut text,
    montant bigint NOT NULL CHECK (montant >= 0),
    sens text NOT NULL CHECK (sens IN ('debit', 'credit')),
    commercant_id bigint,
    categorie_id bigint,
    UNIQUE (releve_id, rang)
);

CREATE INDEX operations_sub_date ON operations (sub, date_operation, id);
CREATE INDEX operations_commercant ON operations (commercant_id) WHERE commercant_id IS NOT NULL;
CREATE INDEX operations_categorie ON operations (categorie_id) WHERE categorie_id IS NOT NULL;
