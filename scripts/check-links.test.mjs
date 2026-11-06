import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { broken, ignored, targets } from './check-links.mjs';

test('targets lit les liens en ligne et les définitions, hors code', () => {
  const md = [
    '[a](a.md) et ![img](img/x.png "titre")',
    '`[pas](code.md)`',
    '```',
    '[pas](bloc.md)',
    '```',
    '[ref]: docs/ref.md',
  ].join('\n');
  assert.deepEqual(
    targets(md).map((t) => t.target),
    ['a.md', 'img/x.png', 'docs/ref.md'],
  );
});

test('broken signale un fichier absent et ignore URLs et ancres', async (t) => {
  const base = await mkdtemp(join(tmpdir(), 'plbls-liens-'));
  t.after(() => rm(base, { recursive: true }));
  await mkdir(join(base, 'docs'));
  await writeFile(join(base, 'docs/ok.md'), '');
  const md = '[ok](ok.md#titre) [dossier](../docs) [racine](/docs/ok.md) [web](https://x.fr) [ancre](#a) [mort](absent.md) [dehors](../../x.md)';
  assert.deepEqual(broken(base, 'docs/index.md', md), ['docs/index.md:1 : absent.md', 'docs/index.md:1 : ../../x.md']);
});

test('les changelogs et les copies vendor sont ignorés', () => {
  assert.ok(ignored('CHANGELOG.md'));
  assert.ok(ignored('changelog.d/x.md'));
  assert.ok(ignored('web/pli/public/vendor/plbls-ui/README.md'));
  assert.ok(!ignored('README.md'));
});
