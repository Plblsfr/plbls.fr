-- Salon « Vidéos » (EST-149) : liens de vidéos publiés en cartes, listes partagées et séances « Regarder ensemble » planifiées.
-- Aucune vidéo n'est stockée : seul le lien et sa source (youtube, twitch, vimeo, dailymotion, file, page) sont conservés.
ALTER TABLE channels DROP CONSTRAINT channels_kind_check;
ALTER TABLE channels ADD CONSTRAINT channels_kind_check CHECK (kind IN ('text', 'voice', 'videos'));

-- Permission « organiser une séance » (bit 27 : 134217728) pour tous les rôles « @tout le monde » existants.
UPDATE roles SET permissions = permissions | 134217728 WHERE is_default;

CREATE TABLE video_posts (
    message_id  bigint PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    channel_id  uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    provider    text NOT NULL,
    ref         text NOT NULL,
    url         text NOT NULL,
    title       text NOT NULL DEFAULT '',
    site        text NOT NULL DEFAULT '',
    thumb       text NOT NULL DEFAULT '',
    watched_at  timestamptz
);
CREATE INDEX video_posts_channel ON video_posts (channel_id, message_id DESC);

CREATE TABLE video_lists (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    channel_id  uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    name        text NOT NULL,
    created_by  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX video_lists_channel ON video_lists (channel_id);

CREATE TABLE video_list_items (
    list_id     uuid   NOT NULL REFERENCES video_lists(id) ON DELETE CASCADE,
    message_id  bigint NOT NULL REFERENCES video_posts(message_id) ON DELETE CASCADE,
    position    int    NOT NULL,
    PRIMARY KEY (list_id, message_id)
);

CREATE TABLE watch_sessions (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    channel_id       uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    voice_channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    title            text NOT NULL,
    starts_at        timestamptz NOT NULL,
    remind_minutes   int  NOT NULL,
    videos           bigint[] NOT NULL DEFAULT '{}',
    created_by       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reminded_at      timestamptz,
    started_at       timestamptz
);
CREATE INDEX watch_sessions_channel ON watch_sessions (channel_id, starts_at);
CREATE INDEX watch_sessions_pending ON watch_sessions (starts_at) WHERE started_at IS NULL;

CREATE TABLE watch_rsvps (
    session_id uuid NOT NULL REFERENCES watch_sessions(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    answer     text NOT NULL CHECK (answer IN ('yes', 'maybe', 'no')),
    PRIMARY KEY (session_id, user_id)
);
