import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { randomBytes } from 'node:crypto';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fetchIdentity, sameOrigin, permissionChecker } from './auth.js';
import { openDb, migrate, insertShare, shareMeta, consumeShare, listShares, deleteShare, purgeExpired } from './db.js';
import { rateLimiter, clientIp } from './limit.js';

const root = fileURLToPath(new URL('..', import.meta.url));
// Dokploy peut laisser les guillemets du .env dans la valeur : on les retire.
const env = (k) => (process.env[k] || '').trim().replace(/^(["'])(.*)\1$/, '$2');
const PORT = Number(process.env.PORT || 8080);
const GATEWAY_URL = env('GATEWAY_URL') || 'https://security.plbls.fr';
const GATEWAY_INTERNAL_URL = env('GATEWAY_INTERNAL_URL') || GATEWAY_URL;
const DEPOSIT_PERMISSION = env('DEPOSIT_PERMISSION') || 'pli:deposit';
const MAX_BYTES = Number(env('MAX_BYTES') || 25 * 1024 * 1024); // taille du chiffré (≈ contenu + 60 octets)
const TTLS = [3600, 86400, 604800, 2592000]; // 1 h, 1 j, 7 j, 30 j
const MAX_PENDING = Number(env('MAX_PENDING_PER_USER') || 50);

const depositConfigured = !!(env('PERMISSION_URL') && env('PERMISSION_SERVICE_TOKEN'));
const canDeposit = depositConfigured ? permissionChecker({
  url: env('PERMISSION_URL'), token: env('PERMISSION_SERVICE_TOKEN'), permission: DEPOSIT_PERMISSION }) : null;
if (!process.env.DATABASE_URL) { console.error('DATABASE_URL manquant (ex. postgres://user:pass@host:5432/pli)'); process.exit(1); }
const db = openDb(process.env.DATABASE_URL);
await migrate(db);
setInterval(() => purgeExpired(db).catch((e) => console.error('purge', e.message)), 60_000).unref();

const readLimit = rateLimiter({ max: 60, windowMs: 60_000 });
const ID_RE = /^[A-Za-z0-9_-]{22}$/;

const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png', '.ico': 'image/x-icon' };
const SECURITY = {
  'content-security-policy': "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'self' https://security.plbls.fr; frame-ancestors 'none'",
  'referrer-policy': 'no-referrer',
  'x-content-type-options': 'nosniff',
  'x-frame-options': 'DENY',
};
const json = (res, code, body) => {
  res.writeHead(code, { 'content-type': 'application/json', 'cache-control': 'no-store', ...SECURITY });
  res.end(JSON.stringify(body));
};

async function rawBody(req, max) {
  const declared = Number(req.headers['content-length']);
  if (declared > max) throw Object.assign(new Error('fichier trop gros'), { status: 413 });
  const chunks = []; let n = 0;
  for await (const c of req) {
    n += c.length;
    if (n > max) throw Object.assign(new Error('fichier trop gros'), { status: 413 });
    chunks.push(c);
  }
  return Buffer.concat(chunks);
}

// Dépôt : session security.plbls.fr + permission « pli:deposit » chez permission.plbls.fr. Renvoie l'utilisateur ou répond.
async function authDeposit(req, res) {
  if (!depositConfigured) { json(res, 503, { error: 'dépôt non configuré (PERMISSION_URL, PERMISSION_SERVICE_TOKEN)' }); return null; }
  const { user, reason } = await fetchIdentity(req.headers.cookie, { gatewayUrl: GATEWAY_INTERNAL_URL });
  if (!user) { console.warn('dépôt 401 :', reason); json(res, 401, { error: 'non authentifié', reason }); return null; }
  if (req.method !== 'GET' && !sameOrigin(req.headers)) { json(res, 403, { error: 'origine refusée' }); return null; }
  const perm = await canDeposit(user.id);
  if (!perm.allowed) {
    console.warn(`dépôt 403 : compte ${user.id}`, perm.reason || `pas de permission ${DEPOSIT_PERMISSION}`);
    json(res, 403, { error: `permission ${DEPOSIT_PERMISSION} requise`, reason: perm.reason, user_id: user.id });
    return null;
  }
  return user;
}

async function api(req, res, path) {
  const method = req.method;
  if (method === 'GET' && path === '/api/config') {
    return json(res, 200, { gatewayUrl: GATEWAY_URL, maxBytes: MAX_BYTES, ttls: TTLS, permission: DEPOSIT_PERMISSION });
  }

  // —— Dépôt (réservé) ——
  if (method === 'GET' && path === '/api/me') {
    const user = await authDeposit(req, res); if (!user) return;
    return json(res, 200, { id: user.id, name: user.name });
  }
  if (method === 'GET' && path === '/api/shares') {
    const user = await authDeposit(req, res); if (!user) return;
    return json(res, 200, await listShares(db, user.id));
  }
  if (method === 'POST' && path === '/api/shares') {
    const user = await authDeposit(req, res); if (!user) return;
    const ttl = Number(new URL(req.url, 'http://x').searchParams.get('ttl'));
    if (!TTLS.includes(ttl)) return json(res, 400, { error: 'durée invalide' });
    if ((await listShares(db, user.id)).length >= MAX_PENDING) return json(res, 429, { error: `${MAX_PENDING} partages en attente au maximum : supprimez-en` });
    const data = await rawBody(req, MAX_BYTES);
    // Format : 0x01 | IV (12) | chiffré AES-GCM (≥ 16 octets de tag). Le serveur ne peut rien vérifier de plus.
    if (data.length < 30 || data[0] !== 1) return json(res, 400, { error: 'contenu chiffré invalide' });
    const id = randomBytes(16).toString('base64url');
    const expires_at = await insertShare(db, { id, owner: user.id, data, ttlSeconds: ttl });
    return json(res, 201, { id, expires_at, size: data.length });
  }
  let m = path.match(/^\/api\/shares\/([^/]+)$/);
  if (method === 'DELETE' && m) {
    const user = await authDeposit(req, res); if (!user) return;
    return json(res, (await deleteShare(db, m[1], user.id)) ? 200 : 404, { ok: true });
  }

  // —— Lecture (publique) ——
  m = path.match(/^\/api\/s\/([^/]+)(\/consume)?$/);
  if (m) {
    if (!readLimit(clientIp(req))) return json(res, 429, { error: 'trop de requêtes, réessayez dans une minute' });
    if (!ID_RE.test(m[1])) return json(res, 404, { error: 'introuvable' });
    // Métadonnées : ne consomment rien (les aperçus de liens et la page peuvent les lire sans risque).
    if (method === 'GET' && !m[2]) {
      const meta = await shareMeta(db, m[1]);
      return meta ? json(res, 200, meta) : json(res, 404, { error: 'introuvable' });
    }
    // Consommation : POST uniquement, jamais déclenchée par un GET (robots, prévisualisations).
    if (method === 'POST' && m[2]) {
      const data = await consumeShare(db, m[1]);
      if (!data) return json(res, 404, { error: 'introuvable' });
      res.writeHead(200, { 'content-type': 'application/octet-stream', 'cache-control': 'no-store', 'content-length': data.length, ...SECURITY });
      return res.end(data);
    }
  }
  json(res, 404, { error: 'introuvable' });
}

async function serveStatic(res, path) {
  if (path === '/') path = '/index.html';
  else if (/^\/s\/[^/]+$/.test(path)) path = '/s.html';
  const base = join(root, 'public');
  const file = normalize(join(base, path));
  if (!file.startsWith(base)) { res.writeHead(403); return res.end(); }
  try {
    const data = await readFile(file);
    res.writeHead(200, { 'content-type': MIME[extname(file)] || 'application/octet-stream', 'cache-control': 'no-cache, max-age=0, must-revalidate', ...SECURITY });
    res.end(data);
  } catch { res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' }); res.end('Introuvable'); }
}

createServer(async (req, res) => {
  const path = new URL(req.url, 'http://x').pathname;
  try {
    if (path.startsWith('/api/')) await api(req, res, path);
    else if (req.method === 'GET') await serveStatic(res, path);
    else { res.writeHead(405); res.end(); }
  } catch (e) {
    const code = e.status || 500;
    if (code === 500) console.error(e);
    if (!res.headersSent) json(res, code, { error: code === 500 ? 'erreur interne' : e.message });
  }
}).listen(PORT, () => console.log(`pli sur :${PORT}`));
