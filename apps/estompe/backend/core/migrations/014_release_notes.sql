-- Notes de version : envoyées en message privé par un compte système, une fois par version.
INSERT INTO users (id, keycloak_sub, pseudo) VALUES ('00000000-0000-0000-0000-000000000001', 'system:estompe', 'Estompe');
CREATE TABLE release_announced (
    version text PRIMARY KEY,
    sent_at timestamptz NOT NULL DEFAULT now()
);
