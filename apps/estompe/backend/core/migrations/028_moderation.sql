-- Modération de serveur (EST-106) : exclusion temporaire, bannissements, pseudo de serveur, mode lent, verrouillage,
-- invitations limitées et journal enrichi (cible et raison).
ALTER TABLE server_members ADD COLUMN timeout_until timestamptz;
ALTER TABLE server_members ADD COLUMN nickname text NOT NULL DEFAULT '';
ALTER TABLE channels ADD COLUMN slowmode int NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN locked boolean NOT NULL DEFAULT false;
ALTER TABLE server_invites ADD COLUMN max_uses int;
ALTER TABLE server_invites ADD COLUMN uses int NOT NULL DEFAULT 0;
ALTER TABLE server_invites ADD COLUMN expires_at timestamptz;
ALTER TABLE audit_log ADD COLUMN target_id uuid;
ALTER TABLE audit_log ADD COLUMN reason text NOT NULL DEFAULT '';
CREATE INDEX audit_log_target ON audit_log (server_id, target_id, id DESC);

CREATE TABLE server_bans (
    server_id  uuid NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    banned_by  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason     text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz,
    PRIMARY KEY (server_id, user_id)
);
