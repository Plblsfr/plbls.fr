CREATE TABLE apps (
    key          text PRIMARY KEY CHECK (key ~ '^[a-z][a-z0-9-]{0,39}$'),
    name         text NOT NULL,
    description  text NOT NULL DEFAULT '',
    manifest     jsonb NOT NULL DEFAULT '{}',
    manifest_sha text NOT NULL DEFAULT '',
    seen_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE resource_types (
    app      text NOT NULL REFERENCES apps(key) ON DELETE CASCADE,
    key      text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_-]{0,39}$'),
    name     text NOT NULL,
    list_url text NOT NULL DEFAULT '',
    PRIMARY KEY (app, key)
);

CREATE TABLE permissions (
    name          text PRIMARY KEY,
    app           text NOT NULL REFERENCES apps(key) ON DELETE CASCADE,
    key           text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_.-]{0,59}$'),
    category      text NOT NULL DEFAULT '',
    label         text NOT NULL,
    description   text NOT NULL DEFAULT '',
    resource_type text,
    open_default  boolean NOT NULL DEFAULT false,
    position      int NOT NULL DEFAULT 0,
    obsolete_at   timestamptz,
    UNIQUE (app, key),
    CHECK (name = app || ':' || key),
    FOREIGN KEY (app, resource_type) REFERENCES resource_types(app, key)
);

CREATE TABLE accounts (
    sub          text PRIMARY KEY CHECK (length(sub) BETWEEN 1 AND 200),
    email        text,
    display_name text,
    last_seen_at timestamptz,
    synced_at    timestamptz,
    in_keycloak  boolean NOT NULL DEFAULT true
);
CREATE INDEX accounts_email ON accounts (lower(email) text_pattern_ops);

CREATE TABLE groups (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
    description text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX groups_name ON groups (lower(name));

CREATE TABLE group_members (
    group_id   uuid NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    sub        text NOT NULL REFERENCES accounts(sub) ON DELETE CASCADE,
    expires_at timestamptz,
    added_by   text NOT NULL,
    added_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, sub)
);
CREATE INDEX group_members_sub ON group_members (sub);

CREATE TABLE roles (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
    description text NOT NULL DEFAULT '',
    system      text UNIQUE CHECK (system IN ('tout-le-monde', 'admin')),
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX roles_name ON roles (lower(name));

CREATE TABLE rules (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_kind text NOT NULL CHECK (subject_kind IN ('role', 'group', 'account')),
    subject_id   text NOT NULL,
    permission   text NOT NULL REFERENCES permissions(name) ON DELETE CASCADE,
    resource     text NOT NULL DEFAULT '*' CHECK (length(resource) BETWEEN 1 AND 200),
    effect       text NOT NULL CHECK (effect IN ('allow', 'deny')),
    expires_at   timestamptz,
    note         text NOT NULL DEFAULT '',
    granted_by   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (subject_kind, subject_id, permission, resource)
);
CREATE INDEX rules_permission ON rules (permission);

CREATE TABLE role_bindings (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    role_id      uuid NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    subject_kind text NOT NULL CHECK (subject_kind IN ('group', 'account')),
    subject_id   text NOT NULL,
    scope_type   text,
    scope_id     text,
    expires_at   timestamptz,
    granted_by   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CHECK ((scope_type IS NULL) = (scope_id IS NULL))
);
CREATE UNIQUE INDEX role_bindings_unique ON role_bindings (role_id, subject_kind, subject_id, coalesce(scope_type, ''), coalesce(scope_id, ''));
CREATE INDEX role_bindings_subject ON role_bindings (subject_kind, subject_id);

CREATE TABLE audit_log (
    id          bigserial PRIMARY KEY,
    at          timestamptz NOT NULL DEFAULT now(),
    actor       text NOT NULL,
    action      text NOT NULL,
    target_kind text NOT NULL,
    target_id   text NOT NULL,
    subject     text,
    detail      jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX audit_log_at ON audit_log (id DESC);
CREATE INDEX audit_log_subject ON audit_log (subject, id DESC);
CREATE INDEX audit_log_actor ON audit_log (actor, id DESC);

CREATE FUNCTION nettoyer_sujet() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM rules WHERE subject_kind = TG_ARGV[0] AND subject_id = OLD.id::text;
    IF TG_ARGV[0] = 'group' THEN
        DELETE FROM role_bindings WHERE subject_kind = 'group' AND subject_id = OLD.id::text;
    END IF;
    RETURN OLD;
END $$;
CREATE TRIGGER roles_nettoyage AFTER DELETE ON roles FOR EACH ROW EXECUTE FUNCTION nettoyer_sujet('role');
CREATE TRIGGER groups_nettoyage AFTER DELETE ON groups FOR EACH ROW EXECUTE FUNCTION nettoyer_sujet('group');

INSERT INTO apps (key, name, description) VALUES ('aegis', 'aegis', 'Administration des permissions');
INSERT INTO permissions (name, app, key, category, label, description)
VALUES ('aegis:admin', 'aegis', 'admin', 'Administration', 'Administrer les permissions', 'Rôles, groupes, droits de tous les comptes');
INSERT INTO roles (name, description, system) VALUES
    ('Tout le monde', 'Donné automatiquement à tout compte connecté', 'tout-le-monde'),
    ('Administrateur aegis', 'Administre les permissions de tous les comptes', 'admin');
INSERT INTO rules (subject_kind, subject_id, permission, effect, granted_by)
SELECT 'role', id::text, 'aegis:admin', 'allow', 'migration' FROM roles WHERE system = 'admin';
