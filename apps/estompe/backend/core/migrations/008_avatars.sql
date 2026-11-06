-- Avatar d'un utilisateur : petite image (PNG ou JPEG) stockée en base ; la version change à chaque modification.
ALTER TABLE users
    ADD COLUMN avatar bytea,
    ADD COLUMN avatar_type text,
    ADD COLUMN avatar_version bigint NOT NULL DEFAULT 0;
