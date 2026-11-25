// Poids du code Rust généré par Slint (out/app.rs), par composant nommé : le code des `if` et des `for` (sous-composants anonymes) est compté
// avec le composant qui les contient. Usage : node scripts/estompe/natif-poids-ui.mjs [chemin/de/app.rs] [nombre de lignes]
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

function trouver() {
  const build = join(import.meta.dirname, '..', '..', 'apps', 'estompe', 'natif', 'target', 'debug', 'build')
  const candidats = readdirSync(build)
    .filter((d) => d.startsWith('estompe-natif-'))
    .map((d) => join(build, d, 'out', 'app.rs'))
    .filter((f) => { try { return statSync(f).isFile() } catch { return false } })
  if (!candidats.length) throw new Error(`aucun app.rs sous ${build} : compiler l'app d'abord`)
  return candidats.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0]
}

const fichier = process.argv[2] || trouver()
const lignes = Number(process.argv[3] ?? 10)
const s = readFileSync(fichier, 'utf8')
const debuts = [...s.matchAll(/struct Inner([A-Za-z_0-9]+)\b/g)].map((m) => [m.index, m[1]])
const taille = new Map()
const parent = new Map()
debuts.forEach(([p, nom], i) => {
  const fin = i + 1 < debuts.length ? debuts[i + 1][0] : s.length
  taille.set(nom, (taille.get(nom) ?? 0) + fin - p)
  for (const [, r] of s.slice(p, fin).matchAll(/Inner(Component_[A-Za-z_0-9]+)\b/g)) {
    if (r !== nom && !parent.has(r)) parent.set(r, nom)
  }
})
const anonyme = (n) => n.startsWith('Component_')
function racine(n) {
  const vu = new Set()
  while (anonyme(n) && parent.has(n) && !vu.has(n)) { vu.add(n); n = parent.get(n) }
  return n.replace(/_root_\d+$/, '')
}
const total = [...taille.values()].reduce((a, b) => a + b, 0)
const parComposant = new Map()
for (const [n, t] of taille) parComposant.set(racine(n), (parComposant.get(racine(n)) ?? 0) + t)
console.log(`${fichier} : ${(total / 1e6).toFixed(1)} Mo, ${parComposant.size} composants`)
console.log('| Composant | Code généré | Part |\n|---|---|---|')
for (const [n, t] of [...parComposant].sort((a, b) => b[1] - a[1]).slice(0, lignes)) {
  console.log(`| \`${n}\` | ${(t / 1e6).toFixed(2)} Mo | ${((100 * t) / total).toFixed(1)} % |`)
}
