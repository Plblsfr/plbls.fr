import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fetchIdentity, sameOrigin, permissionChecker } from './auth.js';

test('identité via /auth/me', async () => {
  let seen;
  const ok = async (url, opts) => { seen = { url, cookie: opts.headers.cookie }; return { ok: true, json: async () => ({ id: 'u1', name: 'a', roles: [] }) }; };
  assert.equal((await fetchIdentity('sid=abc', { gatewayUrl: 'https://g/', fetchFn: ok })).user.id, 'u1');
  assert.deepEqual(seen, { url: 'https://g/auth/me', cookie: 'sid=abc' });
  assert.match((await fetchIdentity('', { gatewayUrl: 'https://g', fetchFn: ok })).reason, /cookie/);
  assert.match((await fetchIdentity('sid=x', { gatewayUrl: 'https://g', fetchFn: async () => ({ ok: false, status: 403 }) })).reason, /403/);
  assert.match((await fetchIdentity('sid=x', { gatewayUrl: 'https://g', fetchFn: async () => { throw new Error('x'); } })).reason, /injoignable/);
});

test('même origine', () => {
  assert.equal(sameOrigin({ origin: 'https://pli.plbls.fr', host: 'pli.plbls.fr' }), true);
  assert.equal(sameOrigin({ origin: 'https://evil.plbls.fr', host: 'pli.plbls.fr' }), false);
  assert.equal(sameOrigin({ host: 'pli.plbls.fr' }), false);
  assert.equal(sameOrigin({ origin: 'https://pli.plbls.fr', host: 'pli:8080', 'x-forwarded-host': 'pli.plbls.fr' }), true);
});

test('permission : autorisé, refusé, cache, erreur = refus', async () => {
  let calls = 0, answer = { ok: true, json: async () => ({ allowed: true }) };
  const check = permissionChecker({ url: 'http://p', token: 't', fetchFn: async () => { calls++; return answer; } });
  assert.equal((await check('u1', 0)).allowed, true);
  assert.equal((await check('u1', 1000)).allowed, true);
  assert.equal(calls, 1);
  answer = { ok: true, json: async () => ({ allowed: false }) };
  assert.equal((await check('u2', 0)).allowed, false);
  const down = permissionChecker({ url: 'http://p', token: 't', fetchFn: async () => { throw new Error('réseau'); } });
  const r = await down('u1');
  assert.equal(r.allowed, false);
  assert.match(r.reason, /injoignable/);
  const unauth = permissionChecker({ url: 'http://p', token: 't', fetchFn: async () => ({ ok: false, status: 401 }) });
  assert.match((await unauth('u1')).reason, /401.*PERMISSION_SERVICE_TOKEN/);
});
