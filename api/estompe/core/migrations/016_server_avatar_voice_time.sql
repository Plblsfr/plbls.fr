-- Photo de profil propre à un serveur (remplace la photo globale dans ce serveur).
CREATE TABLE server_avatars (
    server_id   uuid   NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id     uuid   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    avatar      bytea  NOT NULL,
    avatar_type text   NOT NULL,
    version     bigint NOT NULL DEFAULT 1,
    PRIMARY KEY (server_id, user_id)
);

-- Présence en cours dans un salon vocal (une seule à la fois par personne).
CREATE TABLE voice_sessions (
    user_id    uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    since      timestamptz NOT NULL DEFAULT now()
);

-- Temps cumulé en vocal par membre et par serveur.
CREATE TABLE voice_stats (
    server_id uuid   NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id   uuid   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    seconds   bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (server_id, user_id)
);
