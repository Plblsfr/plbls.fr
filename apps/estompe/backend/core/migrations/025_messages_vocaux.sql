-- Messages vocaux : une pièce jointe de type « voice » porte sa durée (ms) et sa forme d'onde réduite.
ALTER TABLE files
    ADD COLUMN kind text NOT NULL DEFAULT 'file' CHECK (kind IN ('file', 'voice')),
    ADD COLUMN duration_ms integer NOT NULL DEFAULT 0,
    ADD COLUMN waveform integer[] NOT NULL DEFAULT '{}';
