-- Sondages : le texte du message est la question ; choix unique ou multiple, clôture facultative.
CREATE TABLE polls (
    message_id bigint PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    multiple   boolean NOT NULL DEFAULT false,
    closes_at  timestamptz,
    announced_at timestamptz -- résultat publié dans la conversation
);
-- Message posté par le serveur (résultat d'un sondage) ; l'auteur est celui du sondage, mais il n'est pas modifiable.
ALTER TABLE messages ADD COLUMN system boolean NOT NULL DEFAULT false;
CREATE TABLE poll_options (
    id         bigserial PRIMARY KEY,
    message_id bigint NOT NULL REFERENCES polls(message_id) ON DELETE CASCADE,
    label      text   NOT NULL,
    position   int    NOT NULL
);
CREATE INDEX poll_options_message ON poll_options (message_id);
CREATE TABLE poll_votes (
    message_id bigint NOT NULL REFERENCES polls(message_id) ON DELETE CASCADE,
    user_id    uuid   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    option_id  bigint NOT NULL REFERENCES poll_options(id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, user_id, option_id)
);
