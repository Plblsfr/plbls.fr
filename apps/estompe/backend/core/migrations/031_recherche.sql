-- Recherche plein texte des messages : français, insensible aux accents et à la casse (racines des mots).
CREATE EXTENSION IF NOT EXISTS unaccent;
CREATE TEXT SEARCH CONFIGURATION fr (COPY = french);
ALTER TEXT SEARCH CONFIGURATION fr ALTER MAPPING FOR hword, hword_part, word WITH unaccent, french_stem;
ALTER TABLE messages ADD COLUMN tsv tsvector GENERATED ALWAYS AS (to_tsvector('fr', content)) STORED;
CREATE INDEX messages_tsv ON messages USING gin (tsv);
