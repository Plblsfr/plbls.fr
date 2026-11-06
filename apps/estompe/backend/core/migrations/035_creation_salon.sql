-- Création de salon guidée (EST-159) : sujet, limite de participants d'un salon vocal et exceptions par membre (salons privés).
ALTER TABLE channels ADD COLUMN topic text NOT NULL DEFAULT '';
-- 0 : limite du serveur vocal.
ALTER TABLE channels ADD COLUMN user_limit int NOT NULL DEFAULT 0;

CREATE TABLE channel_member_overrides (
    channel_id uuid   NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id    uuid   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    allow      bigint NOT NULL DEFAULT 0,
    deny       bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, user_id)
);
