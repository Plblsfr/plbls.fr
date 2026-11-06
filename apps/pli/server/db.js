import pg from 'pg';

export function openDb(url) {
  return new pg.Pool({ connectionString: url });
}

export async function migrate(db) {
  await db.query(`
    CREATE TABLE IF NOT EXISTS shares (
      id TEXT PRIMARY KEY,
      owner TEXT NOT NULL,
      data BYTEA NOT NULL,
      size INTEGER NOT NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      expires_at TIMESTAMPTZ NOT NULL);
    CREATE INDEX IF NOT EXISTS shares_owner ON shares (owner);
    CREATE INDEX IF NOT EXISTS shares_expires ON shares (expires_at);
  `);
}

// Le serveur ne stocke que le chiffré : ni nom, ni type, ni clé.
export async function insertShare(db, { id, owner, data, ttlSeconds }) {
  const { rows } = await db.query(
    `INSERT INTO shares (id, owner, data, size, expires_at)
     VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5)) RETURNING expires_at`,
    [id, owner, data, data.length, ttlSeconds]);
  return rows[0].expires_at;
}

export async function shareMeta(db, id) {
  const { rows } = await db.query('SELECT size, expires_at FROM shares WHERE id=$1 AND expires_at > now()', [id]);
  return rows[0] || null;
}

// Lecture unique : un seul DELETE … RETURNING, donc une seule requête concurrente obtient le contenu.
export async function consumeShare(db, id) {
  const { rows } = await db.query('DELETE FROM shares WHERE id=$1 AND expires_at > now() RETURNING data', [id]);
  return rows[0]?.data || null;
}

export async function listShares(db, owner) {
  const { rows } = await db.query(
    'SELECT id, size, created_at, expires_at FROM shares WHERE owner=$1 AND expires_at > now() ORDER BY created_at DESC LIMIT 200', [owner]);
  return rows;
}

export async function deleteShare(db, id, owner) {
  const { rowCount } = await db.query('DELETE FROM shares WHERE id=$1 AND owner=$2', [id, owner]);
  return rowCount > 0;
}

export async function purgeExpired(db) {
  const { rowCount } = await db.query('DELETE FROM shares WHERE expires_at <= now()');
  return rowCount;
}
