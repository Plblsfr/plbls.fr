CREATE TABLE dm_conversations (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_a     uuid NOT NULL REFERENCES users(id),
    user_b     uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (user_a < user_b),
    UNIQUE (user_a, user_b)
);
CREATE INDEX dm_conversations_b ON dm_conversations (user_b);

CREATE TABLE messages (
    id         bigserial PRIMARY KEY,
    channel_id uuid REFERENCES channels(id) ON DELETE CASCADE,
    dm_id      uuid REFERENCES dm_conversations(id) ON DELETE CASCADE,
    author_id  uuid NOT NULL REFERENCES users(id),
    content    text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    edited_at  timestamptz,
    CHECK ((channel_id IS NULL) <> (dm_id IS NULL))
);
CREATE INDEX messages_channel ON messages (channel_id, id DESC) WHERE channel_id IS NOT NULL;
CREATE INDEX messages_dm ON messages (dm_id, id DESC) WHERE dm_id IS NOT NULL;

CREATE TABLE message_mentions (
    message_id bigint NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id),
    read_at    timestamptz,
    PRIMARY KEY (message_id, user_id)
);
CREATE INDEX message_mentions_unread ON message_mentions (user_id) WHERE read_at IS NULL;

CREATE TABLE reactions (
    message_id bigint NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id),
    emoji      text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, user_id, emoji)
);
