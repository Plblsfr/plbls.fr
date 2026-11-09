#!/usr/bin/env node
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const FORMAT = 'estompe-mesures/1'
const OPTIONS_TAURI = '--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection'
const SCENARIOS = { session: 'Session entière', repos: 'Repos 5 min', 'gros-salon': "Ouverture d'un gros salon", defilement: 'Défilement automatique 10 s', flux: 'Flux de messages', jeu: 'Partie de jeu avec overlays', reduite: 'Fenêtre réduite' }

const overlays = (s) => Object.entries(s.par_role ?? {}).filter(([r]) => r.startsWith('overlay-')).reduce((t, [, g]) => t + g.prive.mediane_mo, 0)
const METRIQUES = [
  ['Mémoire privée totale, médiane (Mo)', (s) => s.total.prive.mediane_mo],
  ['Mémoire privée totale, pic (Mo)', (s) => s.total.prive.pic_mo],
  ['Working set total, médiane (Mo)', (s) => s.total.ws.mediane_mo],
  ['Working set total, pic (Mo)', (s) => s.total.ws.pic_mo],
  ['CPU total (% d’un cœur)', (s) => s.total.cpu_pct],
  ['Overlays, mémoire privée médiane (Mo)', (s) => (Object.keys(s.par_role ?? {}).some((r) => r.startsWith('overlay-')) ? overlays(s) : null)],
  ['Images : intervalle moyen (ms)', (s) => (s.images.n ? s.images.moyenne_ms : null)],
  ['Images : p95 (ms)', (s) => (s.images.n ? s.images.p95_ms : null)],
  ['Images : p99 (ms)', (s) => (s.images.n ? s.images.p99_ms : null)],
  ['Images perdues (> 25 ms)', (s) => (s.images.n ? s.images.perdues : null)],
  ['Latence message → affiché, médiane (ms)', (s) => (s.latences.messages.n ? s.latences.messages.mediane_ms : null)],
  ['Latence message → affiché, p95 (ms)', (s) => (s.latences.messages.n ? s.latences.messages.p95_ms : null)],
  ['Latence de saisie, médiane (ms)', (s) => (s.latences.saisie.n ? s.latences.saisie.mediane_ms : null)],
  ['Latence de saisie, p95 (ms)', (s) => (s.latences.saisie.n ? s.latences.saisie.p95_ms : null)],
]

export function mediane(valeurs) {
  const v = valeurs.filter((x) => typeof x === 'number' && Number.isFinite(x)).sort((a, b) => a - b)
  if (!v.length) return null
  const m = Math.floor(v.length / 2)
  return v.length % 2 ? v[m] : (v[m - 1] + v[m]) / 2
}

const libelleOptions = (o) => (o.join(' ') === OPTIONS_TAURI ? 'options par défaut' : o.filter((x) => x !== OPTIONS_TAURI).join(' ') || 'options par défaut')
export const cible = (e) => `${e.app} · ${libelleOptions(e.systeme.options_webview2 ?? [])}`

const nombre = (x) => (x === null ? '—' : String(Math.round(x * 10) / 10).replace('.', ','))

export function synthese(exports) {
  const groupes = new Map()
  for (const e of exports) {
    if (e.format !== FORMAT) throw new Error(`format inconnu : ${e.format}`)
    const k = cible(e)
    groupes.set(k, [...(groupes.get(k) ?? []), e])
  }
  const cibles = [...groupes.keys()]
  const entete = (titre) => [`| ${titre} | ${cibles.map((c) => `${c} (${groupes.get(c).length} sessions)`).join(' | ')} |`, `|---|${cibles.map(() => '---:').join('|')}|`]
  const lignes = ['# Synthèse des mesures de l’app Windows', '', 'Chaque valeur est la médiane des sessions de la cible.', '']
  const machines = [...new Set(exports.map((e) => [e.systeme.cpu, `${e.systeme.ram_mo} Mo`, e.systeme.gpu?.join(' + '), e.systeme.windows, `WebView2 ${e.systeme.webview2}`, `échelle ${e.systeme.facteur_echelle}`].filter(Boolean).join(', ')))]
  lignes.push('## Machines', '', ...machines.map((m) => `- ${m}`), '')
  lignes.push('## Démarrage', '', ...entete('Métrique'), `| Lancement → première conversation affichée (ms) | ${cibles.map((c) => nombre(mediane(groupes.get(c).map((e) => e.demarrage_ms)))).join(' | ')} |`, '')
  const ids = ['session', ...Object.keys(SCENARIOS).filter((id) => id !== 'session' && exports.some((e) => e.statistiques.scenarios[id]))]
  for (const id of ids) {
    const stats = (e) => (id === 'session' ? e.statistiques.session : e.statistiques.scenarios[id])
    const rangs = METRIQUES.map(([nom, f]) => [nom, cibles.map((c) => mediane(groupes.get(c).map(stats).filter(Boolean).map(f)))]).filter(([, v]) => v.some((x) => x !== null))
    lignes.push(`## ${SCENARIOS[id]}`, '', ...entete('Métrique'), ...rangs.map(([nom, v]) => `| ${nom} | ${v.map(nombre).join(' | ')} |`), '')
  }
  return lignes.join('\n')
}

function lireTout(chemins) {
  return chemins.flatMap((c) => (statSync(c).isDirectory() ? lireTout(readdirSync(c).filter((f) => f.endsWith('.json')).map((f) => join(c, f))) : [JSON.parse(readFileSync(c, 'utf8'))]))
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2)
  const i = args.indexOf('--sortie')
  const sortie = i >= 0 ? args.splice(i, 2)[1] : null
  if (!args.length) {
    console.error('usage : synthese-mesures.mjs <export.json|dossier>… [--sortie synthese.md]')
    process.exit(2)
  }
  const md = synthese(lireTout(args))
  if (sortie) writeFileSync(sortie, `${md}\n`)
  else console.log(md)
}
