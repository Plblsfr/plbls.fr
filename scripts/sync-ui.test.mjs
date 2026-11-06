import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { compare, distFiles, expectedFiles, write } from './sync-ui.mjs';

async function fakeDist(content = (name) => `/*! @plbls/ui v9.9.9 */\n${name}\n`) {
  const dir = await mkdtemp(join(tmpdir(), 'plbls-dist-'));
  for (const name of distFiles) await writeFile(join(dir, name), content(name));
  return dir;
}

test('une copie écrite par write est conforme', async (t) => {
  const dist = await fakeDist();
  const dest = await mkdtemp(join(tmpdir(), 'plbls-copie-'));
  t.after(() => Promise.all([rm(dist, { recursive: true }), rm(dest, { recursive: true })]));
  const files = await expectedFiles(dist, '9.9.9');
  await write(dest, files);
  assert.deepEqual(await compare(dest, files, 'copie'), []);
  assert.equal(await readFile(join(dest, 'VERSION'), 'utf8'), '9.9.9\n');
});

test('compare signale un fichier modifié, absent ou en trop', async (t) => {
  const dist = await fakeDist();
  const dest = await mkdtemp(join(tmpdir(), 'plbls-copie-'));
  t.after(() => Promise.all([rm(dist, { recursive: true }), rm(dest, { recursive: true })]));
  const files = await expectedFiles(dist, '9.9.9');
  await write(dest, files);
  await writeFile(join(dest, 'plbls-ui.min.css'), 'modifié à la main');
  await rm(join(dest, 'VERSION'));
  await writeFile(join(dest, 'plbls-ui-tokens.css'), 'ancienne copie');
  assert.deepEqual((await compare(dest, files, 'copie')).sort(), [
    'copie/VERSION : absent',
    'copie/plbls-ui-tokens.css : fichier en trop',
    'copie/plbls-ui.min.css : différent de packages/ui',
  ]);
});

test('compare signale un dossier absent', async () => {
  const files = new Map([['VERSION', '1\n']]);
  assert.deepEqual(await compare(join(tmpdir(), 'plbls-inexistant-' + process.pid), files, 'copie'), ['copie : dossier absent']);
});

test('un BOM dans dist est refusé', async (t) => {
  const dist = await fakeDist((name) => `/*! x */\n﻿:root{}${name}`);
  t.after(() => rm(dist, { recursive: true }));
  await assert.rejects(expectedFiles(dist, '9.9.9'), /BOM/);
});
