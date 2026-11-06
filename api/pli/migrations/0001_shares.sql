CREATE TABLE IF NOT EXISTS shares (
  id TEXT PRIMARY KEY,
  owner TEXT NOT NULL,
  data BYTEA NOT NULL,
  size INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL);
CREATE INDEX IF NOT EXISTS shares_owner ON shares (owner);
CREATE INDEX IF NOT EXISTS shares_expires ON shares (expires_at);
