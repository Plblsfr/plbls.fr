// Sans argument (CI, à chaque fusion dans main) : met à jour .version d'après les fusions de main pas encore comptées (nom de la branche, dossiers modifiés),
// recopie la version des services dans leurs manifestes (--manifestes) et affiche .version.
// Fusions pas encore comptées : celles qui suivent, sur la première lignée de main, le dernier commit qui a modifié .version (fusion de la PR de version ou modification à la main).
// Un service absent de .version part de la version de son manifeste.
// Avec --manifestes : écrit la version de chaque service de .version dans api/<service>/Cargo.toml, Cargo.lock, packages/ui/package.json et package-lock.json.
// Avec --fichiers : liste les fichiers que la PR de version peut modifier.
// Avec --apply (build des apps) : écrit les versions de .version dans les tauri.conf.json, Cargo.toml et Cargo.lock de apps/estompe/{windows,android}.
// Les chemins sont relatifs à la racine du dépôt.
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { pendingVersions, SERVICES_RUST } from './version.mjs'

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim()
process.chdir(fileURLToPath(new URL('../..', import.meta.url)))
const lire = (p) => readFileSync(p, 'utf8')
const lireJson = (p) => JSON.parse(lire(p))
const ecrireJson = (p, v) => writeFileSync(p, JSON.stringify(v, null, 2) + '\n')
const remplacer = (p, re, v) => writeFileSync(p, lire(p).replace(re, (_, pre) => `${pre}"${v}"`))
const echapper = (s) => s.replace(/[.*+?^${}()|[\]\\-]/g, '\\$&')

const UI = 'packages/ui'
const cargo = (s) => `api/${s}/Cargo.toml`
const paquet = (s) => lire(cargo(s)).match(/^name = "(.*)"/m)[1]
const FICHIERS = ['.version', ...SERVICES_RUST.map(cargo), 'Cargo.lock', `${UI}/package.json`, 'package-lock.json']

function versionManifeste(service) {
  if (service === 'ui') return lireJson(`${UI}/package.json`).version
  return lire(cargo(service)).match(/^version = "(.*)"/m)[1]
}

function ecrireManifestes(versions) {
  for (const s of SERVICES_RUST) {
    remplacer(cargo(s), /^(version = )".*"/m, versions[s])
    remplacer('Cargo.lock', new RegExp(`(\\nname = "${echapper(paquet(s))}"\\nversion = )".*"`), versions[s])
  }
  ecrireJson(`${UI}/package.json`, { ...lireJson(`${UI}/package.json`), version: versions.ui })
  const verrou = lireJson('package-lock.json')
  verrou.packages[UI].version = versions.ui
  ecrireJson('package-lock.json', verrou)
}

const versions = lireJson('.version')

if (process.argv.includes('--fichiers')) {
  console.log(FICHIERS.join('\n'))
} else if (process.argv.includes('--manifestes')) {
  ecrireManifestes(versions)
} else if (process.argv.includes('--apply')) {
  for (const app of ['windows', 'android']) {
    const v = versions[app]
    const path = `apps/estompe/${app}/src-tauri/tauri.conf.json`
    ecrireJson(path, { ...lireJson(path), version: v })
    for (const [file, re] of [['Cargo.toml', /^(version = )".*"/m], ['Cargo.lock', /(name = "estompe"\nversion = )".*"/]]) {
      remplacer(`apps/estompe/${app}/src-tauri/${file}`, re, v)
    }
    console.log(`${app} ${v}`)
  }
} else {
  const depart = { ...versions }
  for (const s of [...SERVICES_RUST, 'ui']) depart[s] ??= versionManifeste(s)
  const depuis = git('log', '-1', '--first-parent', '--format=%H', 'HEAD', '--', '.version') || 'HEAD^1'
  const commits = git('rev-list', '--first-parent', '--reverse', `${depuis}..HEAD`).split('\n').filter(Boolean)
  const next = pendingVersions(depart, commits.map((c) => ({
    subject: git('log', '-1', '--format=%s', c),
    files: git('diff', '--name-only', `${c}^1`, c).split('\n').filter(Boolean),
  })))
  ecrireJson('.version', next)
  ecrireManifestes(next)
  console.log(JSON.stringify(next))
}
