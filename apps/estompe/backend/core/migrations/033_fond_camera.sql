-- Fond personnalisé de la caméra d'une personne : image (PNG ou JPEG) retrouvée sur tous ses appareils ; visible d'elle seule.
ALTER TABLE users
    ADD COLUMN camera_bg bytea,
    ADD COLUMN camera_bg_type text;
