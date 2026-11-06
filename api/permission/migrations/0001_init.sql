-- Modèle RBAC par application.
-- permission = (application, clé)  ex. chat + message.delete  -> "chat:message.delete"
-- rôle       = paquet de permissions d'UNE application
-- assignment = rôle donné à un compte (sub Keycloak)

CREATE TABLE apps (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    key         text NOT NULL UNIQUE CHECK (key ~ '^[a-z][a-z0-9-]{0,39}$'),
    name        text NOT NULL,
    description text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE permissions (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    app_id      uuid NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    key         text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_.-]{0,59}$'),
    description text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (app_id, key),
    UNIQUE (id, app_id)
);

CREATE TABLE roles (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    app_id      uuid NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    key         text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_.-]{0,59}$'),
    name        text NOT NULL,
    description text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (app_id, key),
    UNIQUE (id, app_id)
);

-- app_id dupliqué pour que la base interdise un rôle portant la permission d'une autre application.
CREATE TABLE role_permissions (
    role_id       uuid NOT NULL,
    permission_id uuid NOT NULL,
    app_id        uuid NOT NULL,
    PRIMARY KEY (role_id, permission_id),
    FOREIGN KEY (role_id, app_id)       REFERENCES roles(id, app_id)       ON DELETE CASCADE,
    FOREIGN KEY (permission_id, app_id) REFERENCES permissions(id, app_id) ON DELETE CASCADE
);

CREATE TABLE assignments (
    user_id    text NOT NULL,
    role_id    uuid NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    granted_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, role_id)
);
CREATE INDEX assignments_role_idx ON assignments (role_id);

-- Journal des modifications (qui a changé quoi).
CREATE TABLE audit_log (
    id         bigserial PRIMARY KEY,
    at         timestamptz NOT NULL DEFAULT now(),
    actor      text NOT NULL,
    action     text NOT NULL,
    target     text NOT NULL,
    detail     jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX audit_log_at_idx ON audit_log (at DESC);
