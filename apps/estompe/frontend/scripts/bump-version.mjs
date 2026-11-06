// Sans argument (CI, à chaque fusion dans main) : met à jour .version d'après le dernier commit (nom de la branche, dossiers modifiés) et l'affiche.
// Avec --apply (build des apps) : écrit les versions de .version dans les tauri.conf.json, Cargo.toml et Cargo.lock de windows/ et android/.
// Les chemins sont relatifs à apps/estompe (le monorepo contient d'autres projets).
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { nextVersions } from './version.mjs'

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim()
process.chdir(fileURLToPath(new URL('../..', import.meta.url)))
const versions = JSON.parse(readFileSync('.version', 'utf8'))

if (process.argv.includes('--apply')) {
  for (const app of ['windows', 'android']) {
    const v = versions[app]
    const path = `${app}/src-tauri/tauri.conf.json`
    writeFileSync(path, JSON.stringify({ ...JSON.parse(readFileSync(path, 'utf8')), version: v }, null, 2) + '\n')
    for (const [file, re] of [['Cargo.toml', /^(version = )".*"/m], ['Cargo.lock', /(name = "estompe"\nversion = )".*"/]]) {
      const p = `${app}/src-tauri/${file}`
      writeFileSync(p, readFileSync(p, 'utf8').replace(re, (_, pre) => `${pre}"${v}"`))
    }
    console.log(`${app} ${v}`)
  }
} else {
  // Un commit de fusion a deux parents : on compare au premier (main avant la fusion) ; un commit simple, à son parent.
  const files = git('diff', '--relative', '--name-only', 'HEAD^1', 'HEAD').split('\n').filter(Boolean)
  const next = nextVersions(versions, git('log', '-1', '--format=%s'), files)
  writeFileSync('.version', JSON.stringify(next, null, 2) + '\n')
  console.log(JSON.stringify(next))
}
