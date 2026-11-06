-- Espace d'administration : suspension des comptes, journal des actions et erreurs serveur.
ALTER TABLE users ADD COLUMN suspended_at timestamptz;
CREATE TABLE admin_log (
  id     bigserial PRIMARY KEY,
  at     timestamptz NOT NULL DEFAULT now(),
  actor  text NOT NULL,
  action text NOT NULL,
  target text NOT NULL DEFAULT ''
);
CREATE TABLE server_errors (
  id     bigserial PRIMARY KEY,
  at     timestamptz NOT NULL DEFAULT now(),
  method text NOT NULL,
  path   text NOT NULL,
  status int NOT NULL
);
