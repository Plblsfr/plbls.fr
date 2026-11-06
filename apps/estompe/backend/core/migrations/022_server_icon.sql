-- Icône d'un serveur : petite image (PNG ou JPEG) stockée en base ; la version change à chaque modification, 0 = pas d'icône.
ALTER TABLE servers
    ADD COLUMN icon bytea,
    ADD COLUMN icon_type text,
    ADD COLUMN icon_version bigint NOT NULL DEFAULT 0;
