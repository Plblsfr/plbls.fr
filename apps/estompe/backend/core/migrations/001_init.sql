CREATE TABLE users (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    keycloak_sub text NOT NULL UNIQUE,
    pseudo      text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_pseudo_lower ON users (lower(pseudo));

CREATE TABLE invitations (
    code        text PRIMARY KEY,
    created_at  timestamptz NOT NULL DEFAULT now(),
    used_by     uuid REFERENCES users(id),
    used_at     timestamptz,
    revoked_at  timestamptz
);
