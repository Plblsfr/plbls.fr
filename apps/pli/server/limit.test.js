import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rateLimiter } from './limit.js';

test('limiteur de débit : fenêtre fixe par clé', () => {
  const ok = rateLimiter({ max: 2, windowMs: 1000 });
  assert.equal(ok('a', 0), true);
  assert.equal(ok('a', 1), true);
  assert.equal(ok('a', 2), false);
  assert.equal(ok('b', 2), true);
  assert.equal(ok('a', 1001), true);
});
