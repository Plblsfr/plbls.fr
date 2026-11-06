// Test de bout en bout du serveur : vrai Postgres (TEST_DATABASE_URL, base jetable), gateway et permission simulés.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';

const DB = process.env.TEST_DATABASE_URL;
const skip = !DB && 'TEST_DATABASE_URL absent';
let fake, app, base, appPort, fakePort;
const blob = (n = 40) => { const b = Buffer.alloc(n, 7); b[0] = 1; return b; };
const as = (who) => ({ cookie: `sid=${who}`, origin: base });

before(async () => {
  if (skip) return;
  fake = createServer((req, res) => {
    let body = ''; req.on('data', (c) => body += c);
    req.on('end', () => {
      res.setHeader('content-type', 'application/json');
      if (req.url === '/auth/me') {
        const who = /sid=(\w+)/.exec(req.headers.cookie || '')?.[1];
        return who ? res.end(JSON.stringify({ id: who, name: who })) : (res.statusCode = 401, res.end('{}'));
      }
      if (req.url === '/v1/check') {
        assert.equal(req.headers.authorization, 'Bearer tok');
        const { user_id, permission } = JSON.parse(body);
        return res.end(JSON.stringify({ allowed: user_id === 'alice' && permission === 'pli:deposit' }));
      }
      res.statusCode = 404; res.end('{}');
    });
  });
  await new Promise((r) => fake.listen(0, '127.0.0.1', r));
  fakePort = fake.address().port;
  appPort = 18000 + Math.floor(Math.random() * 1000);
  base = `http://127.0.0.1:${appPort}`;
  app = spawn('node', ['server/index.js'], { env: { ...process.env, PORT: appPort, DATABASE_URL: DB,
    GATEWAY_INTERNAL_URL: `http://127.0.0.1:${fakePort}`, PERMISSION_URL: `http://127.0.0.1:${fakePort}`, PERMISSION_SERVICE_TOKEN: 'tok', MAX_BYTES: '1000', MAX_PENDING_PER_USER: '3' }, stdio: ['ignore', 'pipe', 'inherit'] });
  await new Promise((ok, ko) => { app.stdout.on('data', (d) => /pli sur/.test(d) && ok()); app.on('exit', () => ko(new Error('serveur arrêté'))); });
});
after(() => { app?.kill(); fake?.close(); });

const post = (path, body, headers = {}) => fetch(base + path, { method: 'POST', body, headers: { 'content-type': 'application/octet-stream', ...headers } });

test('dépôt : 401 sans session, 403 sans permission, refus d\'une autre origine', { skip }, async () => {
  assert.equal((await post('/api/shares?ttl=3600', blob(), { origin: base })).status, 401);
  assert.equal((await post('/api/shares?ttl=3600', blob(), as('bob'))).status, 403);
  assert.equal((await post('/api/shares?ttl=3600', blob(), { cookie: 'sid=alice', origin: 'https://evil.example' })).status, 403);
  assert.equal((await post('/api/shares?ttl=3600', blob(), { cookie: 'sid=alice' })).status, 403);
});

test('dépôt : validation (durée, format, taille)', { skip }, async () => {
  assert.equal((await post('/api/shares?ttl=5', blob(), as('alice'))).status, 400);
  assert.equal((await post('/api/shares?ttl=3600', Buffer.from('en clair, trop court'), as('alice'))).status, 400);
  assert.equal((await post('/api/shares?ttl=3600', blob(1001), as('alice'))).status, 413);
});

test('lecture unique : méta sans consommer, un seul POST réussit, lecture publique', { skip }, async () => {
  const data = blob(60);
  const r = await post('/api/shares?ttl=3600', data, as('alice'));
  assert.equal(r.status, 201);
  const { id } = await r.json();
  assert.match(id, /^[A-Za-z0-9_-]{22}$/);
  // la page et les métadonnées se lisent sans session et plusieurs fois
  assert.equal((await fetch(`${base}/s/${id}`)).status, 200);
  for (let i = 0; i < 2; i++) assert.equal((await (await fetch(`${base}/api/s/${id}`)).json()).size, 60);
  // un GET ne consomme pas
  assert.equal((await fetch(`${base}/api/s/${id}/consume`)).status, 404);
  assert.equal((await fetch(`${base}/api/s/${id}`)).status, 200);
  // deux lectures simultanées : une seule obtient le contenu
  const res = await Promise.all([post(`/api/s/${id}/consume`), post(`/api/s/${id}/consume`)]);
  assert.deepEqual(res.map((x) => x.status).sort(), [200, 404]);
  const got = res.find((x) => x.status === 200);
  assert.deepEqual(Buffer.from(await got.arrayBuffer()), data);
  assert.equal((await fetch(`${base}/api/s/${id}`)).status, 404);
  assert.equal((await post(`/api/s/${id}/consume`)).status, 404);
});

test('identifiants invalides, liste, révocation par le propriétaire seulement, quota', { skip }, async () => {
  assert.equal((await fetch(`${base}/api/s/nimportequoi`)).status, 404);
  const ids = [];
  for (let i = 0; i < 3; i++) ids.push((await (await post('/api/shares?ttl=86400', blob(), as('alice'))).json()).id);
  assert.equal((await post('/api/shares?ttl=86400', blob(), as('alice'))).status, 429);
  const list = await (await fetch(`${base}/api/shares`, { headers: as('alice') })).json();
  assert.deepEqual(new Set(list.map((s) => s.id)), new Set(ids));
  assert.ok(!('data' in list[0]));
  assert.equal((await fetch(`${base}/api/shares/${ids[0]}`, { method: 'DELETE', headers: as('bob') })).status, 403);
  assert.equal((await fetch(`${base}/api/shares/${ids[0]}`, { method: 'DELETE', headers: as('alice') })).status, 200);
  assert.equal((await fetch(`${base}/api/s/${ids[0]}`)).status, 404);
  for (const i of ids.slice(1)) await fetch(`${base}/api/shares/${i}`, { method: 'DELETE', headers: as('alice') });
});

test('en-têtes de sécurité et pas de cache sur l\'API', { skip }, async () => {
  const r = await fetch(`${base}/`);
  assert.match(r.headers.get('content-security-policy'), /frame-ancestors 'none'/);
  assert.equal((await fetch(`${base}/api/config`)).headers.get('cache-control'), 'no-store');
  assert.equal((await fetch(`${base}/..%2fserver/index.js`)).status, 404);
});
