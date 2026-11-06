CREATE TABLE servers (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name       text NOT NULL,
    owner_id   uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE server_members (
    server_id uuid NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id   uuid NOT NULL REFERENCES users(id),
    joined_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (server_id, user_id)
);
CREATE INDEX server_members_user ON server_members (user_id);

CREATE TABLE channels (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    server_id  uuid NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name       text NOT NULL,
    kind       text NOT NULL CHECK (kind IN ('text', 'voice')),
    position   serial NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX channels_name ON channels (server_id, kind, lower(name));

-- Lien d'invitation à un serveur : réutilisable jusqu'à révocation.
CREATE TABLE server_invites (
    code       text PRIMARY KEY,
    server_id  uuid NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz
);
