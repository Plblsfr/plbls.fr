// Sans argument (CI, à chaque fusion dans main) : met à jour .version d'après les fusions de main pas encore comptées (nom de la branche, dossiers modifiés) et l'affiche.
// Fusions pas encore comptées : celles qui suivent, sur la première lignée de main, le dernier commit qui a modifié .version (fusion de la PR de version ou modification à la main).
// Avec --apply (build des apps) : écrit les versions de .version dans les tauri.conf.json, Cargo.toml et Cargo.lock de apps/estompe/{windows,android}.
// Les chemins sont relatifs à la racine du dépôt.
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { pendingVersions } from './version.mjs'

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim()
process.chdir(fileURLToPath(new URL('../..', import.meta.url)))
const versions = JSON.parse(readFileSync('.version', 'utf8'))

if (process.argv.includes('--apply')) {
  for (const app of ['windows', 'android']) {
    const v = versions[app]
    const path = `apps/estompe/${app}/src-tauri/tauri.conf.json`
    writeFileSync(path, JSON.stringify({ ...JSON.parse(readFileSync(path, 'utf8')), version: v }, null, 2) + '\n')
    for (const [file, re] of [['Cargo.toml', /^(version = )".*"/m], ['Cargo.lock', /(name = "estompe"\nversion = )".*"/]]) {
      const p = `apps/estompe/${app}/src-tauri/${file}`
      writeFileSync(p, readFileSync(p, 'utf8').replace(re, (_, pre) => `${pre}"${v}"`))
    }
    console.log(`${app} ${v}`)
  }
} else {
  const depuis = git('log', '-1', '--first-parent', '--format=%H', 'HEAD', '--', '.version') || 'HEAD^1'
  const commits = git('rev-list', '--first-parent', '--reverse', `${depuis}..HEAD`).split('\n').filter(Boolean)
  const next = pendingVersions(versions, commits.map((c) => ({
    subject: git('log', '-1', '--format=%s', c),
    files: git('diff', '--name-only', `${c}^1`, c).split('\n').filter(Boolean),
  })))
  writeFileSync('.version', JSON.stringify(next, null, 2) + '\n')
  console.log(JSON.stringify(next))
}
