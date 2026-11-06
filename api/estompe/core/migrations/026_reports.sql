-- Signalements de bugs : envoyés par les utilisateurs, traités dans l'espace d'administration.
CREATE TABLE reports (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id    uuid NOT NULL REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now(),
    title        text NOT NULL,
    description  text NOT NULL,
    category     text NOT NULL DEFAULT '',
    platform     text NOT NULL DEFAULT '',
    tech         jsonb NOT NULL DEFAULT '{}',
    logs         text NOT NULL DEFAULT '',
    status       text NOT NULL DEFAULT 'new' CHECK (status IN ('new', 'in_progress', 'resolved', 'rejected', 'duplicate')),
    duplicate_of uuid REFERENCES reports(id) ON DELETE SET NULL,
    notes        text NOT NULL DEFAULT '',
    seen         boolean NOT NULL DEFAULT false
);
CREATE INDEX reports_author ON reports (author_id, created_at DESC);
-- SET NULL : les captures d'un signalement supprimé sont ensuite purgées par le nettoyage des orphelins.
ALTER TABLE files ADD COLUMN report_id uuid REFERENCES reports(id) ON DELETE SET NULL;
