CREATE TABLE evenements (
    id text PRIMARY KEY,
    sub text NOT NULL,
    source text NOT NULL,
    type text NOT NULL,
    date timestamptz NOT NULL,
    importe boolean NOT NULL DEFAULT false,
    donnees jsonb NOT NULL,
    recu_le timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX evenements_sub_date ON evenements (sub, date);
CREATE INDEX evenements_date ON evenements (date);

CREATE TABLE agregats_jour (
    sub text NOT NULL,
    jour date NOT NULL,
    stat text NOT NULL,
    cle text NOT NULL DEFAULT '',
    valeur bigint NOT NULL,
    PRIMARY KEY (sub, jour, stat, cle)
);

CREATE INDEX agregats_jour_sub_stat_jour ON agregats_jour (sub, stat, jour);

CREATE TABLE battements (
    sub text PRIMARY KEY,
    dernier timestamptz NOT NULL
);
