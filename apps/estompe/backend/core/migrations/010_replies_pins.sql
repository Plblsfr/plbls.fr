-- Réponses (citation d'un message) et messages épinglés.
ALTER TABLE messages
    ADD COLUMN reply_to  bigint REFERENCES messages(id) ON DELETE SET NULL,
    ADD COLUMN pinned_at timestamptz;
CREATE INDEX messages_pinned_channel ON messages (channel_id, pinned_at DESC) WHERE pinned_at IS NOT NULL AND channel_id IS NOT NULL;
CREATE INDEX messages_pinned_dm ON messages (dm_id, pinned_at DESC) WHERE pinned_at IS NOT NULL AND dm_id IS NOT NULL;
