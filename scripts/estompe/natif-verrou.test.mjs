import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';

const verrou = new URL('./natif-verrou.sh', import.meta.url).pathname;
const flock = spawnSync('sh', ['-c', 'command -v flock']).status === 0;

function lancer(dossier, nom, attente) {
  const commande = `echo debut-${nom} >> journal; ${attente ? `until [ -e ${attente} ]; do sleep 0.05; done;` : ''} echo fin-${nom} >> journal`;
  const p = spawn(verrou, ['sh', '-c', commande], {
    cwd: dossier,
    env: { ...process.env, ESTOMPE_NATIF_VERROU: '', ESTOMPE_NATIF_VERROU_FICHIER: join(dossier, 'verrou') },
  });
  p.stderrTexte = '';
  p.stderr.on('data', (d) => { p.stderrTexte += d; p.emit('stderr'); });
  return p;
}

async function attendre(p, motif) {
  while (!motif.test(p.stderrTexte)) await once(p, 'stderr');
}

async function journal(dossier) {
  return (await readFile(join(dossier, 'journal'), 'utf8').catch(() => '')).trim().split('\n').filter(Boolean);
}

test('le second build attend la fin du premier en le disant', { skip: !flock && 'flock absent' }, async (t) => {
  const dossier = await mkdtemp(join(tmpdir(), 'natif-verrou-'));
  t.after(() => rm(dossier, { recursive: true }));
  const premier = lancer(dossier, 'premier', 'libere');
  while (!(await journal(dossier)).includes('debut-premier')) await new Promise((r) => setImmediate(r));
  const second = lancer(dossier, 'second');
  await attendre(second, /attente de sa fin/);
  assert.deepEqual(await journal(dossier), ['debut-premier']);
  await writeFile(join(dossier, 'libere'), '');
  const [[codePremier], [codeSecond]] = await Promise.all([once(premier, 'exit'), once(second, 'exit')]);
  assert.equal(codePremier, 0);
  assert.equal(codeSecond, 0);
  assert.deepEqual(await journal(dossier), ['debut-premier', 'fin-premier', 'debut-second', 'fin-second']);
});

test('un build lancé sous le verrou ne le reprend pas', { skip: !flock && 'flock absent' }, async (t) => {
  const dossier = await mkdtemp(join(tmpdir(), 'natif-verrou-'));
  t.after(() => rm(dossier, { recursive: true }));
  const env = { ...process.env, ESTOMPE_NATIF_VERROU: '', ESTOMPE_NATIF_VERROU_FICHIER: join(dossier, 'verrou') };
  const r = spawnSync(verrou, [verrou, 'sh', '-c', 'echo imbrique'], { cwd: dossier, env, encoding: 'utf8', timeout: 10000 });
  assert.equal(r.status, 0);
  assert.equal(r.stdout.trim(), 'imbrique');
});
