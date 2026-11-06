CREATE TABLE files (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    uploader_id uuid NOT NULL REFERENCES users(id),
    -- SET NULL : le contenu d'un message supprimé est ensuite purgé par le nettoyage des orphelins.
    message_id  bigint REFERENCES messages(id) ON DELETE SET NULL,
    name        text NOT NULL,
    mime        text NOT NULL,
    size        bigint NOT NULL,
    key         text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX files_message ON files (message_id) WHERE message_id IS NOT NULL;
CREATE INDEX files_orphans ON files (created_at) WHERE message_id IS NULL;
