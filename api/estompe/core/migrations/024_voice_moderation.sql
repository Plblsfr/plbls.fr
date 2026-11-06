-- Modération vocale (EST-102) : micro ou son coupés par le propriétaire, par salon et par participant, et journal par serveur.
CREATE TABLE voice_mod (
  channel_id uuid    NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
  user_id    uuid    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  mic_off    boolean NOT NULL DEFAULT false,
  sound_off  boolean NOT NULL DEFAULT false,
  PRIMARY KEY (channel_id, user_id)
);
CREATE TABLE voice_mod_log (
  id         bigserial PRIMARY KEY,
  at         timestamptz NOT NULL DEFAULT now(),
  server_id  uuid    NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
  channel_id uuid    NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
  actor      uuid    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  target     uuid    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  mic_off    boolean NOT NULL,
  sound_off  boolean NOT NULL
);
