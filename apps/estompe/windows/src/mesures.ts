import type { DonneesSonde, Point } from '../../../../web/estompe/src/lib/sonde'

export const FORMAT_MESURES = 'estompe-mesures/1'
export const PAUSE_MS = 1000
export const IMAGE_PERDUE_MS = 25
const MO = 1024 * 1024

export interface MesureProcessus { pid: number; type: string; role: string; prive: number; ws: number; pic_ws: number; cpu_ms: number }
export interface Echantillon { t: number; processus: MesureProcessus[] }
export interface Releve { debut: number; fin: number; intervalle_ms: number; echantillons: Echantillon[] }
export interface Systeme {
  app: string; webview2: string | null; windows: string | null; cpu: string | null; coeurs: number; ram_mo: number | null
  gpu: string[]; facteur_echelle: number | null; options_webview2: string[]; demarrage_processus: number | null
}
export interface Marqueur { nom: string; debut: number; fin: number }

export interface Scenario { id: string; titre: string; duree: number; images: boolean; auto?: 'defilement' | 'reduire'; aide: string }
export const SCENARIOS: Scenario[] = [
  { id: 'repos', titre: 'Repos 5 min', duree: 300_000, images: false, aide: 'Ne touchez à rien pendant 5 minutes, Estompe ouverte sur un salon texte.' },
  { id: 'gros-salon', titre: "Ouverture d'un gros salon", duree: 30_000, images: true, aide: 'Dans les 30 secondes, ouvrez un salon au long historique et remontez-y.' },
  { id: 'defilement', titre: 'Défilement automatique 10 s', duree: 10_000, images: true, auto: 'defilement', aide: 'Fermez les paramètres : après 3 secondes, le salon ouvert défile seul pendant 10 secondes.' },
  { id: 'flux', titre: 'Flux de messages', duree: 60_000, images: true, aide: 'Pendant 1 minute, laissez arriver des messages dans le salon ouvert.' },
  { id: 'jeu', titre: 'Partie de jeu avec overlays', duree: 300_000, images: false, aide: 'Lancez un jeu et restez dans un salon vocal 5 minutes, overlays affichés.' },
  { id: 'reduite', titre: 'Fenêtre réduite', duree: 120_000, images: false, auto: 'reduire', aide: 'La fenêtre est réduite pendant 2 minutes, puis rouverte.' },
]

export interface StatsMemoire { mediane_mo: number; pic_mo: number }
export interface StatsGroupe { prive: StatsMemoire; ws: StatsMemoire; cpu_pct: number }
export interface StatsImages { n: number; moyenne_ms: number; p95_ms: number; p99_ms: number; perdues: number }
export interface StatsLatences { n: number; mediane_ms: number; p95_ms: number; max_ms: number }
export interface StatsFenetre {
  duree_s: number; echantillons: number; total: StatsGroupe; par_role: Record<string, StatsGroupe>; par_type: Record<string, StatsGroupe>
  images: StatsImages; latences: { messages: StatsLatences; saisie: StatsLatences }
}

const arrondi = (x: number, n = 1) => Math.round(x * 10 ** n) / 10 ** n

export function quantile(valeurs: number[], q: number): number {
  if (!valeurs.length) return 0
  const v = [...valeurs].sort((a, b) => a - b)
  const i = (v.length - 1) * q
  const bas = Math.floor(i)
  return v[bas] + (v[Math.min(bas + 1, v.length - 1)] - v[bas]) * (i - bas)
}
export const mediane = (v: number[]) => quantile(v, 0.5)

export function statsImages(intervalles: number[]): StatsImages {
  const v = intervalles.filter((x) => x > 0 && x <= PAUSE_MS)
  const moyenne = v.length ? v.reduce((a, b) => a + b, 0) / v.length : 0
  return { n: v.length, moyenne_ms: arrondi(moyenne, 2), p95_ms: arrondi(quantile(v, 0.95), 2), p99_ms: arrondi(quantile(v, 0.99), 2), perdues: v.filter((x) => x > IMAGE_PERDUE_MS).length }
}

export function statsLatences(v: number[]): StatsLatences {
  return { n: v.length, mediane_ms: arrondi(mediane(v), 2), p95_ms: arrondi(quantile(v, 0.95), 2), max_ms: arrondi(v.length ? Math.max(...v) : 0, 2) }
}

function statsGroupe(echantillons: Echantillon[], garder: (p: MesureProcessus) => boolean): StatsGroupe {
  const prive = echantillons.map((e) => e.processus.filter(garder).reduce((s, p) => s + p.prive, 0) / MO)
  const ws = echantillons.map((e) => e.processus.filter(garder).reduce((s, p) => s + p.ws, 0) / MO)
  const bornes = new Map<number, { t0: number; c0: number; c1: number }>()
  for (const e of echantillons) for (const p of e.processus.filter(garder)) {
    const b = bornes.get(p.pid)
    if (b) b.c1 = p.cpu_ms
    else bornes.set(p.pid, { t0: e.t, c0: p.cpu_ms, c1: p.cpu_ms })
  }
  const duree = echantillons.length > 1 ? echantillons[echantillons.length - 1].t - echantillons[0].t : 0
  const cpu = [...bornes.values()].reduce((s, b) => s + Math.max(0, b.c1 - b.c0), 0)
  return {
    prive: { mediane_mo: arrondi(mediane(prive)), pic_mo: arrondi(prive.length ? Math.max(...prive) : 0) },
    ws: { mediane_mo: arrondi(mediane(ws)), pic_mo: arrondi(ws.length ? Math.max(...ws) : 0) },
    cpu_pct: duree > 0 ? arrondi((cpu / duree) * 100) : 0,
  }
}

const dans = (debut: number, fin: number) => ([t]: Point) => t >= debut && t <= fin

export function statsFenetre(echantillons: Echantillon[], sonde: DonneesSonde, debut: number, fin: number): StatsFenetre {
  const e = echantillons.filter((x) => x.t >= debut && x.t <= fin)
  const roles = [...new Set(e.flatMap((x) => x.processus.map((p) => p.role)))].sort()
  const types = [...new Set(e.flatMap((x) => x.processus.map((p) => p.type)))].sort()
  const garde = dans(debut, fin)
  return {
    duree_s: arrondi((fin - debut) / 1000),
    echantillons: e.length,
    total: statsGroupe(e, () => true),
    par_role: Object.fromEntries(roles.map((r) => [r, statsGroupe(e, (p) => p.role === r)])),
    par_type: Object.fromEntries(types.map((t) => [t, statsGroupe(e, (p) => p.type === t)])),
    images: statsImages(sonde.images.filter(garde).map(([, ms]) => ms)),
    latences: { messages: statsLatences(sonde.messages.filter(garde).map(([, ms]) => ms)), saisie: statsLatences(sonde.saisie.filter(garde).map(([, ms]) => ms)) },
  }
}

export interface ExportMesures {
  format: string; cree: string; app: string; systeme: Omit<Systeme, 'app'>; demarrage_ms: number | null
  debut: number; fin: number; intervalle_ms: number; marqueurs: Marqueur[]; echantillons: Echantillon[]; sonde: DonneesSonde
  statistiques: { session: StatsFenetre; scenarios: Record<string, StatsFenetre> }
}

export function construireExport(releve: Releve, systeme: Systeme, sonde: DonneesSonde, marqueurs: Marqueur[], premiereConversation: number | null, maintenant = Date.now()): ExportMesures {
  const { app, ...reste } = systeme
  const demarrage = premiereConversation !== null && systeme.demarrage_processus !== null && premiereConversation >= systeme.demarrage_processus ? premiereConversation - systeme.demarrage_processus : null
  return {
    format: FORMAT_MESURES,
    cree: new Date(maintenant).toISOString(),
    app,
    systeme: reste,
    demarrage_ms: demarrage,
    debut: releve.debut,
    fin: releve.fin,
    intervalle_ms: releve.intervalle_ms,
    marqueurs,
    echantillons: releve.echantillons,
    sonde,
    statistiques: {
      session: statsFenetre(releve.echantillons, sonde, releve.debut, releve.fin),
      scenarios: Object.fromEntries(marqueurs.map((m) => [m.nom, statsFenetre(releve.echantillons, sonde, m.debut, m.fin)])),
    },
  }
}

export function echantillonsCsv(echantillons: Echantillon[]): string {
  const lignes = ['t,pid,type,role,prive_octets,ws_octets,pic_ws_octets,cpu_ms']
  for (const e of echantillons) for (const p of e.processus) lignes.push([e.t, p.pid, p.type, p.role, p.prive, p.ws, p.pic_ws, p.cpu_ms].join(','))
  return `${lignes.join('\n')}\n`
}
