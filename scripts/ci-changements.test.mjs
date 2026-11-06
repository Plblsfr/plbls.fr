import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const script = join(dirname(fileURLToPath(import.meta.url)), 'ci-changements.sh');

async function depot(t, modifies) {
  const dir = await mkdtemp(join(tmpdir(), 'plbls-changements-'));
  t.after(() => rm(dir, { recursive: true }));
  const git = (...args) => execFileSync('git', args, { cwd: dir, encoding: 'utf8' });
  git('init', '-q', '-b', 'main');
  git('config', 'user.email', 'test@plbls.fr');
  git('config', 'user.name', 'test');
  await writeFile(join(dir, 'base.txt'), 'base\n');
  git('add', '.');
  git('commit', '-qm', 'base');
  git('switch', '-qc', 'branche');
  for (const fichier of modifies) {
    await mkdir(join(dir, dirname(fichier)), { recursive: true });
    await writeFile(join(dir, fichier), 'modifié\n');
  }
  git('add', '.');
  git('commit', '-qm', 'branche');
  git('switch', '-q', 'main');
  git('merge', '-q', '--no-ff', '-m', 'fusion', 'branche');
  return dir;
}

async function lancer(dir, evenement, chemins, ref = 'refs/pull/1/merge') {
  const sortie = join(dir, '.sortie');
  await writeFile(sortie, '');
  execFileSync('bash', [script, ...chemins], {
    cwd: dir,
    env: { ...process.env, GITHUB_EVENT_NAME: evenement, GITHUB_REF: ref, GITHUB_OUTPUT: sortie },
    stdio: ['ignore', 'ignore', 'inherit'],
  });
  return (await readFile(sortie, 'utf8')).trim();
}

const backend = ['api/estompe/**', '!api/estompe/**/*.md', 'Cargo.toml'];

test('un fichier du dossier suivi déclenche les jobs', async (t) => {
  const dir = await depot(t, ['api/estompe/core/src/lib.rs']);
  assert.equal(await lancer(dir, 'pull_request', backend), 'modifie=true');
});

test('un fichier exclu ne déclenche rien, à la racine du dossier comme plus bas', async (t) => {
  const dir = await depot(t, ['api/estompe/README.md', 'api/estompe/core/NOTES.md']);
  assert.equal(await lancer(dir, 'pull_request', backend), 'modifie=false');
});

test('un fichier hors des chemins ne déclenche rien', async (t) => {
  const dir = await depot(t, ['web/pli/public/index.js', 'web/estompe/Cargo.toml']);
  assert.equal(await lancer(dir, 'pull_request', backend), 'modifie=false');
});

test('un fichier racine nommé exactement est suivi', async (t) => {
  const dir = await depot(t, ['Cargo.toml']);
  assert.equal(await lancer(dir, 'pull_request', backend), 'modifie=true');
});

test('un push sur une branche compare le commit de fusion à son premier parent', async (t) => {
  const dir = await depot(t, ['web/pli/public/index.js']);
  assert.equal(await lancer(dir, 'push', backend, 'refs/heads/main'), 'modifie=false');
  assert.equal(await lancer(dir, 'push', ['web/pli/**'], 'refs/heads/main'), 'modifie=true');
});

test('un tag ou un lancement manuel lance tout', async (t) => {
  const dir = await depot(t, ['web/pli/public/index.js']);
  assert.equal(await lancer(dir, 'push', backend, 'refs/tags/pli-v1.0.0'), 'modifie=true');
  assert.equal(await lancer(dir, 'workflow_dispatch', backend, 'refs/heads/main'), 'modifie=true');
});

test('--nom choisit la clé de sortie', async (t) => {
  const dir = await depot(t, ['web/pli/public/index.js']);
  assert.equal(await lancer(dir, 'pull_request', ['--nom', 'pli', 'web/pli/**']), 'pli=true');
  assert.equal(await lancer(dir, 'workflow_dispatch', ['--nom', 'rust', 'Cargo.toml'], 'refs/heads/main'), 'rust=true');
});
