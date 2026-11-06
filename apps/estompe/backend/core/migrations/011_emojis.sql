-- Emojis personnalisés d'un serveur (petites images, utilisables avec :nom: dans les messages).
CREATE TABLE custom_emojis (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    server_id  uuid NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name       text NOT NULL,
    mime       text NOT NULL,
    data       bytea NOT NULL,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX custom_emojis_name ON custom_emojis (server_id, lower(name));
