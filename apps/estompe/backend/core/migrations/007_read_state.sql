-- Dernier message lu par utilisateur et par conversation (salon texte ou message privé).
CREATE TABLE read_states (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id uuid REFERENCES channels(id) ON DELETE CASCADE,
    dm_id      uuid REFERENCES dm_conversations(id) ON DELETE CASCADE,
    last_id    bigint NOT NULL,
    CHECK ((channel_id IS NULL) <> (dm_id IS NULL))
);
CREATE UNIQUE INDEX read_states_channel ON read_states (user_id, channel_id) WHERE channel_id IS NOT NULL;
CREATE UNIQUE INDEX read_states_dm ON read_states (user_id, dm_id) WHERE dm_id IS NOT NULL;
