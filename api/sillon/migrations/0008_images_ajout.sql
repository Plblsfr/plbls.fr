CREATE TABLE images_ajout (
    id text PRIMARY KEY,
    url text NOT NULL,
    vue_le timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX images_ajout_vue_le ON images_ajout (vue_le);

ALTER TABLE ajouts ADD COLUMN image text;
