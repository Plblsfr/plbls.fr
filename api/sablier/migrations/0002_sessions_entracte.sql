CREATE TABLE sessions_entracte (
    sub text PRIMARY KEY,
    jour date NOT NULL,
    debut timestamptz NOT NULL,
    dernier timestamptz NOT NULL,
    secondes bigint NOT NULL,
    episodes bigint NOT NULL DEFAULT 0
);
