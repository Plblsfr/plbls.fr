-- Transcription écrite des messages vocaux : calculée une fois par le serveur, corrigeable par l'auteur, cherchable.
ALTER TABLE files
    ADD COLUMN transcript text NOT NULL DEFAULT '',
    ADD COLUMN transcript_auto boolean NOT NULL DEFAULT false,
    ADD COLUMN transcript_tsv tsvector GENERATED ALWAYS AS (to_tsvector('fr', transcript)) STORED;
CREATE INDEX files_transcript_tsv ON files USING gin (transcript_tsv) WHERE transcript <> '';
