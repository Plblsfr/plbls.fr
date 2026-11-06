-- Rôles de serveur (EST-104) : permissions par rôle, exceptions par salon et journal d'administration.
CREATE TABLE roles (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    server_id   uuid NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name        text NOT NULL,
    color       text NOT NULL DEFAULT '',
    icon        text NOT NULL DEFAULT '',
    position    int  NOT NULL,
    hoist       boolean NOT NULL DEFAULT false,
    permissions bigint NOT NULL DEFAULT 0,
    is_default  boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX roles_default ON roles (server_id) WHERE is_default;
CREATE UNIQUE INDEX roles_name ON roles (server_id, lower(name));

CREATE TABLE member_roles (
    server_id uuid NOT NULL,
    user_id   uuid NOT NULL,
    role_id   uuid NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id),
    FOREIGN KEY (server_id, user_id) REFERENCES server_members(server_id, user_id) ON DELETE CASCADE
);
CREATE INDEX member_roles_server ON member_roles (server_id);

CREATE TABLE channel_overrides (
    channel_id uuid   NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    role_id    uuid   NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    allow      bigint NOT NULL DEFAULT 0,
    deny       bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, role_id)
);

CREATE TABLE audit_log (
    id        bigserial PRIMARY KEY,
    at        timestamptz NOT NULL DEFAULT now(),
    server_id uuid NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    actor     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    action    text NOT NULL,
    target    text NOT NULL DEFAULT ''
);
CREATE INDEX audit_log_server ON audit_log (server_id, id DESC);

-- « @tout le monde » reproduit ce que peuvent faire aujourd'hui les membres (valeur de roles.Everyone).
INSERT INTO roles (server_id, name, position, permissions, is_default) SELECT id, '@tout le monde', 0, 25677568, true FROM servers;
