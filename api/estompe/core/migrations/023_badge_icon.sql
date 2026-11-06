-- Badges : icône image (SVG nettoyé, WebP, PNG ou JPEG) stockée en base, version incrémentée à chaque changement (0 = pas d'image : l'emoji `icon` sert de repli), ordre d'affichage.
ALTER TABLE badges
    ADD COLUMN icon_data bytea,
    ADD COLUMN icon_type text,
    ADD COLUMN icon_version bigint NOT NULL DEFAULT 0,
    ADD COLUMN sort_order int NOT NULL DEFAULT 0;
