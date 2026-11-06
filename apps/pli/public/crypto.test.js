import { test } from 'node:test';
import assert from 'node:assert/strict';
import { encryptPayload, decryptPayload, validKey, safeName } from './crypto.js';

test('aller-retour texte', async () => {
  const msg = new TextEncoder().encode('bonjour é 🔐');
  const { blob, key } = await encryptPayload({ kind: 'text' }, msg);
  assert.equal(blob[0], 1);
  assert.ok(validKey(key));
  const out = await decryptPayload(blob, key);
  assert.deepEqual(out.header, { kind: 'text' });
  assert.equal(new TextDecoder().decode(out.content), 'bonjour é 🔐');
});

test('le blob ne contient ni le nom ni le contenu en clair', async () => {
  const { blob } = await encryptPayload({ kind: 'file', name: 'secret-rapport.pdf' }, new TextEncoder().encode('CONFIDENTIEL'));
  const s = Buffer.from(blob).toString('latin1');
  assert.ok(!s.includes('secret-rapport') && !s.includes('CONFIDENTIEL'));
});

test('mauvaise clé et contenu altéré refusés', async () => {
  const { blob, key } = await encryptPayload({ kind: 'text' }, new Uint8Array([1, 2, 3]));
  const other = (await encryptPayload({ kind: 'text' }, new Uint8Array([1]))).key;
  await assert.rejects(decryptPayload(blob, other), /impossible/);
  const bad = blob.slice(); bad[bad.length - 1] ^= 1;
  await assert.rejects(decryptPayload(bad, key), /impossible/);
  await assert.rejects(decryptPayload(blob, 'court'), /invalide/);
});

test('fichier vide et nom sûr', async () => {
  const { blob, key } = await encryptPayload({ kind: 'file', name: 'a' }, new Uint8Array(0));
  assert.equal((await decryptPayload(blob, key)).content.length, 0);
  assert.ok(!/[\\/]/.test(safeName('../../etc/passwd')));
  assert.equal(safeName(''), 'fichier');
  assert.equal(safeName('a\nb'), 'a_b');
});
