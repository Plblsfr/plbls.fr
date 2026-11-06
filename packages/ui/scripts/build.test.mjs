import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFile, readdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
execFileSync(process.execPath, [join(root, 'scripts/build.mjs')], { stdio: 'ignore' });
const pkg = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));

test('dist contient les fichiers attendus', async () => {
  const files = (await readdir(dist)).sort();
  assert.deepEqual(files, [
    'plbls-ui.css',
    'plbls-ui.esm.d.ts',
    'plbls-ui.esm.js',
    'plbls-ui.esm.min.d.ts',
    'plbls-ui.esm.min.js',
    'plbls-ui.js',
    'plbls-ui.min.css',
    'tokens.css',
    'tokens.min.css',
  ]);
});

test('aucun fichier de dist ne contient de BOM et tous portent la bannière de version', async () => {
  for (const name of await readdir(dist)) {
    const text = await readFile(join(dist, name), 'utf8');
    assert.ok(!text.includes('﻿'), `BOM dans ${name}`);
    assert.ok(text.startsWith(`/*! @plbls/ui v${pkg.version} `), `bannière absente de ${name}`);
  }
});

test('les jetons minifiés commencent par la règle :root', async () => {
  for (const name of ['plbls-ui.min.css', 'tokens.min.css']) {
    const [, second] = (await readFile(join(dist, name), 'utf8')).split('\n');
    assert.ok(second.startsWith(':root{--creme'), name);
  }
});

test('les types déclarent chaque export du module', async () => {
  const index = await readFile(join(root, 'src/js/index.js'), 'utf8');
  const dts = await readFile(join(root, 'src/js/index.d.ts'), 'utf8');
  const names = [...index.matchAll(/export \{([^}]+)\}/g)].flatMap((m) => m[1].split(',').map((s) => s.trim()));
  assert.ok(names.length > 10);
  for (const name of names) assert.match(dts, new RegExp(`export (const|function|class) ${name}\\b`), name);
});
