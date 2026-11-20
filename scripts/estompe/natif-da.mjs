#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const racine = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const JETONS = 'web/estompe/src/styles/v3-jetons.css'
const ICONES = 'web/estompe/src/components/Icon.svelte'
const SORTIE = 'apps/estompe/natif/ui/genere'

function bloc(css, selecteur) {
  const debut = css.indexOf(selecteur)
  if (debut < 0) throw new Error(`${JETONS} : sélecteur « ${selecteur} » introuvable`)
  const ouvre = css.indexOf('{', debut)
  return css.slice(ouvre + 1, css.indexOf('}', ouvre))
}

function declarations(texte) {
  const jetons = new Map()
  for (const m of texte.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)) jetons.set(m[1], m[2].trim())
  return jetons
}

function couleur(valeur, jetons, nom) {
  const hex = valeur.match(/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i)
  if (hex) return hex[1].length === 3 ? `#${[...hex[1]].map((c) => c + c).join('')}`.toLowerCase() : valeur.toLowerCase()
  if (valeur === 'transparent') return 'transparent'
  const ref = valeur.match(/^var\(--([a-z0-9-]+)\)$/)
  if (ref) {
    if (!jetons.has(ref[1])) throw new Error(`--${nom} : var(--${ref[1]}) inconnu`)
    return couleur(jetons.get(ref[1]), jetons, ref[1])
  }
  const rgb = valeur.match(/^rgb\((\d+) (\d+) (\d+)(?: \/ (\d+)%)?\)$/)
  if (rgb) {
    const [r, v, b] = rgb.slice(1, 4).map((n) => Number(n).toString(16).padStart(2, '0'))
    const alpha = rgb[4] === undefined ? '' : Math.round((Number(rgb[4]) * 255) / 100).toString(16).padStart(2, '0')
    return `#${r}${v}${b}${alpha}`
  }
  return null
}

const HORS_COULEUR = new Set(['ombre', 'o1', 'o2', 'o3'])

export function genererCouleurs(css) {
  const clair = declarations(bloc(css, ":root[data-theme='light']"))
  const sombre = declarations(bloc(css, ":root[data-theme='dark']"))
  const lignes = []
  for (const [nom, valeur] of clair) {
    if (HORS_COULEUR.has(nom)) continue
    if (!sombre.has(nom)) throw new Error(`--${nom} : absent du thème sombre`)
    const c = couleur(valeur, clair, nom)
    const s = couleur(sombre.get(nom), sombre, nom)
    if (!c || !s) throw new Error(`--${nom} : valeur non reconnue (${valeur} / ${sombre.get(nom)})`)
    lignes.push(`    out property <color> ${nom}: sombre ? ${s} : ${c};`)
  }
  for (const nom of sombre.keys()) if (!clair.has(nom)) throw new Error(`--${nom} : absent du thème clair`)
  return `import { Palette } from "std-widgets.slint";

export enum ModeTheme { auto, clair, sombre }

export global Theme {
    in-out property <ModeTheme> mode: ModeTheme.auto;
    in-out property <bool> systeme-sombre: Palette.color-scheme == ColorScheme.dark;
    out property <bool> sombre: mode == ModeTheme.sombre || (mode == ModeTheme.auto && systeme-sombre);
${lignes.join('\n')}
    out property <color> calque: encre.mix(surface, 0.07);
}
`
}

const n = (x) => String(Math.round(x * 1000) / 1000)

function forme(balise) {
  const attr = (nom) => {
    const m = balise.match(new RegExp(`\\s${nom}="([^"]*)"`))
    return m ? m[1] : undefined
  }
  if (balise.startsWith('<path')) return attr('d')
  if (balise.startsWith('<circle')) {
    const [cx, cy, r] = ['cx', 'cy', 'r'].map((a) => Number(attr(a)))
    return `M${n(cx - r)} ${n(cy)}a${n(r)} ${n(r)} 0 1 0 ${n(2 * r)} 0a${n(r)} ${n(r)} 0 1 0 ${n(-2 * r)} 0z`
  }
  if (balise.startsWith('<rect')) {
    const [x, y, l, h] = ['x', 'y', 'width', 'height'].map((a) => Number(attr(a)))
    const r = Number(attr('rx') ?? 0)
    if (!r) return `M${n(x)} ${n(y)}h${n(l)}v${n(h)}h${n(-l)}z`
    const arc = (dx, dy) => `a${n(r)} ${n(r)} 0 0 1 ${n(dx)} ${n(dy)}`
    return `M${n(x + r)} ${n(y)}h${n(l - 2 * r)}${arc(r, r)}v${n(h - 2 * r)}${arc(-r, r)}h${n(2 * r - l)}${arc(-r, -r)}v${n(2 * r - h)}${arc(r, -r)}z`
  }
  throw new Error(`${ICONES} : balise non gérée ${balise}`)
}

function table(source, nom) {
  const m = source.match(new RegExp(`const ${nom} = \\{([\\s\\S]*?)\\n  \\}`))
  if (!m) throw new Error(`${ICONES} : table « ${nom} » introuvable`)
  const entrees = new Map()
  for (const e of m[1].matchAll(/^\s*(?:'([^']+)'|([a-z0-9]+)):\s*'([^']*)',?$/gm)) entrees.set(e[1] ?? e[2], e[3])
  return entrees
}

function ensemble(source, nom) {
  const m = source.match(new RegExp(`const ${nom} = new Set<string>\\(\\[([^\\]]*)\\]\\)`))
  if (!m) throw new Error(`${ICONES} : ensemble « ${nom} » introuvable`)
  return [...m[1].matchAll(/'([^']+)'/g)].map((x) => x[1])
}

export function genererIcones(source) {
  const traces = new Map([...table(source, 'v3'), ...table(source, 'propres')])
  const alias = table(source, 'alias')
  const chemins = new Map()
  for (const [nom, svg] of traces) {
    const balises = svg.match(/<[a-z]+[^>]*\/>/g)
    if (!balises) throw new Error(`${ICONES} : icône « ${nom} » sans tracé`)
    chemins.set(nom, balises.map(forme).join(''))
  }
  for (const [nom, cible] of alias) {
    if (!chemins.has(cible)) throw new Error(`${ICONES} : alias « ${nom} » vers « ${cible} » inconnu`)
  }
  const noms = [...chemins.keys()].sort()
  const cas = (liste, rendu) => liste.map(rendu).join('\n')
  const parmi = (nom) => {
    const liste = ensemble(source, nom)
    return liste.length ? liste.map((x) => `cle == "${x}"`).join(' || ') : 'false'
  }
  return `export global Icones {
    out property <[string]> noms: [${noms.map((x) => `"${x}"`).join(', ')}];
    pure public function cle(nom: string) -> string {
${cas([...alias], ([a, c]) => `        if (nom == "${a}") { return "${c}"; }`)}
        return nom;
    }
    pure public function trace(nom: string) -> string {
${cas(noms, (x) => `        if (nom == "${x}") { return "${chemins.get(x)}"; }`)}
        return "";
    }
    pure public function pleine(cle: string) -> bool {
        return ${parmi('pleins')};
    }
    pure public function epaisse(cle: string) -> bool {
        return ${parmi('epais')};
    }
}
`
}

function principal() {
  const verifier = process.argv.includes('--check')
  const fichiers = [
    ['couleurs.slint', genererCouleurs(readFileSync(join(racine, JETONS), 'utf8'))],
    ['icones.slint', genererIcones(readFileSync(join(racine, ICONES), 'utf8'))],
  ]
  let ecart = false
  for (const [nom, contenu] of fichiers) {
    const chemin = join(racine, SORTIE, nom)
    if (!verifier) {
      mkdirSync(dirname(chemin), { recursive: true })
      writeFileSync(chemin, contenu)
      console.log(`écrit : ${SORTIE}/${nom}`)
      continue
    }
    let actuel = null
    try {
      actuel = readFileSync(chemin, 'utf8')
    } catch {}
    if (actuel !== contenu) {
      ecart = true
      console.error(`${SORTIE}/${nom} ne correspond plus à sa source : lancer « node scripts/estompe/natif-da.mjs » et versionner le résultat.`)
    }
  }
  if (ecart) process.exit(1)
  if (verifier) console.log('natif-da : jetons de couleur et icônes à jour')
}

if (process.argv[1] === fileURLToPath(import.meta.url)) principal()
