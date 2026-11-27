import { dateCourte, euros, eurosSignes, moisLong, nombre, periode } from './formats'
import { filtres, type Filtres, type Nature, type Operation } from './tirelire'

export type Mode = 'calendar' | 'statement'
export const MODES: { id: Mode; label: string }[] = [{ id: 'statement', label: 'Par relevé' }, { id: 'calendar', label: 'Mois calendaire' }]
export const CLE_MODE = 'tirelire.mode'

export interface Periode { id: string; from: string; to: string; days: number; known_through: string; elapsed_days: number; complete: boolean; open: boolean; statement_ids: number[] }
export interface ResumePeriode extends Periode { operations: number; expenses: number; income: number; savings: number; remaining: number }
export interface Ecart { amount: number; delta: number; delta_bp: number | null }
export interface Moyenne extends Ecart { periods: number }
export interface LigneCategorie {
  id: number; name: string; kind: Nature; color: number; archived: boolean; monthly_cap: number | null; amount: number; operations: number; share_bp: number | null
  previous: Ecart | null; avg3: Moyenne | null; avg12: Moyenne | null; cap_delta: number | null; cap_bp: number | null; trend: number[]
}
export interface Depassement { category_id: number; reference: 'cap' | 'average'; reference_amount: number; periods: number | null; excess: number; excess_bp: number | null; rank_amount: number; rank_percent: number }
export interface Projection { total: number; variable: number; fixed: number; fixed_pending: number; daily_variable: number; remaining_days: number; budget_delta: number | null }
export interface Budget { total: number; spent: number; bp: number | null; delta: number; uncapped: number }
export interface PointHistorique { id: string; from: string; to: string; complete: boolean; expenses: number; income: number; savings: number; remaining: number; savings_rate_bp: number | null }
export type RaisonProjection = 'complete' | 'partial' | 'too_early'

export interface Mois {
  mode: Mode
  period: Periode
  previous_id: string | null
  next_id: string | null
  periods: number
  operations: number
  expenses: { total: number; fixed: number; variable: number; unclassified: number; fixed_bp: number | null; variable_bp: number | null; unclassified_bp: number | null; previous: Ecart | null; previous_to_date: Ecart | null; avg3: Moyenne | null; avg12: Moyenne | null }
  income: { total: number; classified: number; unclassified: number; unclassified_operations: number; previous: Ecart | null }
  savings: { total: number; rate_bp: number | null; previous: Ecart | null }
  remaining: { total: number; previous: Ecart | null }
  budget: Budget | null
  categories: LigneCategorie[]
  unclassified: { amount: number; operations: number; share_bp: number | null; previous: Ecart | null }
  overruns: Depassement[]
  overrun_total: number
  cumulative: { current: number[]; previous: { id: string; values: number[] } | null; average: { periods: number; values: number[] } | null }
  projection: Projection | null
  projection_reason: RaisonProjection | null
  history: PointHistorique[]
}

export interface LigneCommercant { merchant_id: number | null; name: string | null; amount: number; operations: number; average: number; previous: Ecart | null }
export interface DetailCategorie {
  mode: Mode
  period: Periode
  previous_id: string | null
  next_id: string | null
  periods: number
  category: LigneCategorie
  history: { id: string; from: string; to: string; complete: boolean; amount: number }[]
  over_cap: number
  merchants: LigneCommercant[]
  merchants_total: number
  operations: Operation[]
  operations_total: number
}

const INSECABLE = '\u00a0'
const MOINS = '\u2212'
const majuscule = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

export const lireMode = (v: string | null | undefined): Mode => (v === 'statement' ? 'statement' : 'calendar')

export const eurosRonds = (centimes: number) => `${centimes < 0 ? MOINS : ''}${nombre(Math.round(Math.abs(centimes) / 100))}${INSECABLE}€`

export const eurosRondsSignes = (centimes: number) => `${centimes < 0 ? MOINS : '+'}${eurosRonds(Math.abs(centimes))}`

export function pourcent(bp: number, decimales = 0): string {
  const facteur = 10 ** decimales
  const v = Math.round((Math.abs(bp) / 100) * facteur) / facteur
  const texte = decimales ? v.toFixed(decimales).replace('.', ',') : String(v)
  return `${bp < 0 && v !== 0 ? MOINS : ''}${texte}${INSECABLE}%`
}

export const pourcentSigne = (bp: number) => (Math.round(Math.abs(bp) / 100) === 0 ? `=${INSECABLE}0${INSECABLE}%` : `${bp > 0 ? '+' : ''}${pourcent(bp)}`)

export type Sens = 'hausse' | 'baisse' | 'egal'
export const sensDe = (delta: number): Sens => (delta > 0 ? 'hausse' : delta < 0 ? 'baisse' : 'egal')
export const FLECHES: Record<Sens, string> = { hausse: '▲', baisse: '▼', egal: '=' }
export const MOTS_SENS: Record<Sens, string> = { hausse: 'en hausse', baisse: 'en baisse', egal: 'stable' }

export const ecartCourt = (e: Ecart) => (e.delta_bp === null ? (e.delta === 0 ? `=${INSECABLE}0${INSECABLE}%` : eurosSignes(e.delta)) : pourcentSigne(e.delta_bp))
export const sensEcart = (e: Ecart): Sens => (e.delta_bp !== null && Math.round(Math.abs(e.delta_bp) / 100) === 0 ? 'egal' : sensDe(e.delta))

const debutDe = (id: string) => (id.length === 7 ? `${id}-01` : id)
export const moisDe = (id: string) => moisLong(debutDe(id)).replace(/ \d{4}$/, '')

export const titrePeriode = (p: Pick<Periode, 'from' | 'to'>, mode: Mode) => (mode === 'calendar' ? majuscule(moisLong(p.from)) : `Relevé du ${dateCourte(p.to)}`)
export const nomPeriode = (p: Pick<Periode, 'id' | 'from' | 'to'>, mode: Mode) => (mode === 'calendar' ? majuscule(moisLong(p.from)) : `${dateCourte(p.from)} → ${dateCourte(p.to)}`)
export const nomPrecedente = (id: string | null | undefined, mode: Mode) => (mode === 'statement' ? 'relevé précédent' : id ? moisDe(id) : 'mois précédent')
export const unite = (mode: Mode, n = 1) => (mode === 'calendar' ? 'mois' : n > 1 ? 'relevés' : 'relevé')
export const moyenneSur = (m: Moyenne, mode: Mode) => `moyenne sur ${m.periods} ${unite(mode, m.periods)}`

export function etatPeriode(p: Periode, mode: Mode): string {
  if (mode === 'statement') return `${periode(p.from, p.to)} · ${p.days} jours`
  if (p.open) return `Jour ${p.elapsed_days} sur ${p.days} · données jusqu’au ${dateCourte(p.known_through)}`
  if (p.complete) return `${periode(p.from, p.to)} · mois complet`
  return `${periode(p.from, p.to)} · mois incomplet : tes relevés ne le couvrent pas en entier`
}

export function filtresPeriode(p: Periode, mode: Mode, f: Partial<Filtres> = {}): Filtres {
  if (mode === 'statement' && p.statement_ids.length === 1) return filtres({ releve: p.statement_ids[0], ...f })
  return filtres({ du: p.from, au: p.to, parAchat: mode === 'calendar', ...f })
}

export function parRang(depassements: Depassement[], en: 'euros' | 'pourcent'): Depassement[] {
  return depassements.toSorted((a, b) => (en === 'euros' ? a.rank_amount - b.rank_amount : a.rank_percent - b.rank_percent))
}

export interface Graduations { max: number; pas: number; valeurs: number[] }

export function graduations(maximum: number, cible = 3): Graduations {
  const brut = Math.max(maximum, 100) / cible
  const puissance = 10 ** Math.floor(Math.log10(brut))
  const pas = [1, 2, 2.5, 5, 10].map((k) => k * puissance).find((k) => k >= brut) ?? 10 * puissance
  const max = Math.ceil(Math.max(maximum, 100) / pas) * pas
  const valeurs: number[] = []
  for (let v = pas; v <= max + 0.5; v += pas) valeurs.push(Math.round(v))
  return { max, pas, valeurs }
}

export interface Cadre { largeur: number; hauteur: number; gauche: number; droite: number; haut: number; bas: number }
export const abscisse = (c: Cadre, jours: number) => (jour: number) => c.gauche + ((jour - 1) / Math.max(1, jours - 1)) * (c.largeur - c.gauche - c.droite)
export const ordonnee = (c: Cadre, max: number) => (valeur: number) => c.hauteur - c.bas - (Math.max(0, valeur) / Math.max(1, max)) * (c.hauteur - c.haut - c.bas)

export const trace = (valeurs: number[], x: (jour: number) => number, y: (valeur: number) => number) => valeurs.map((v, i) => `${i ? 'L' : 'M'}${x(i + 1).toFixed(1)} ${y(v).toFixed(1)}`).join('')

export function joursGradues(jours: number, dernier?: number): number[] {
  const res = [1]
  for (let j = 5; j < jours - 2; j += 5) res.push(j)
  res.push(jours)
  return dernier && dernier < jours ? res.filter((j) => Math.abs(j - dernier) > 2).concat(dernier).sort((a, b) => a - b) : res
}

export function etincelle(valeurs: number[], largeur: number, hauteur: number, marge = 3): { chemin: string; x: number; y: number } | null {
  if (valeurs.length < 2) return null
  const min = Math.min(...valeurs)
  const max = Math.max(...valeurs)
  const x = (i: number) => marge + (i / (valeurs.length - 1)) * (largeur - 2 * marge)
  const y = (v: number) => (max === min ? hauteur / 2 : hauteur - marge - ((v - min) / (max - min)) * (hauteur - 2 * marge))
  return { chemin: valeurs.map((v, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(''), x: x(valeurs.length - 1), y: y(valeurs.at(-1)!) }
}

export interface Jauge { plein: number; exces: number; repere: number | null }

export function jauge(montant: number, reference: number | null): Jauge {
  const m = Math.max(0, montant)
  if (reference === null || reference <= 0) return { plein: m > 0 ? 100 : 0, exces: 0, repere: null }
  const echelle = Math.max(m, reference) * 1.04
  return { plein: (Math.min(m, reference) / echelle) * 100, exces: (Math.max(0, m - reference) / echelle) * 100, repere: (reference / echelle) * 100 }
}

export const ANNEAU = 2 * Math.PI * 50
export const arcs = (bp: number) => ({ plein: (Math.min(Math.max(bp, 0), 10_000) / 10_000) * ANNEAU, exces: (Math.min(Math.max(bp - 10_000, 0), 10_000) / 10_000) * ANNEAU })

export function referenceDe(d: Depassement, mode: Mode): string {
  return d.reference === 'cap' ? `plafond ${euros(d.reference_amount)}` : `moyenne ${euros(d.reference_amount)} sur ${d.periods ?? 0} ${unite(mode, d.periods ?? 0)}`
}

export function phraseProjection(m: Mois): string {
  const p = m.projection
  if (!p) return ''
  const fixes = p.fixed_pending > 0 ? `, plus ${eurosRonds(p.fixed_pending)} de charges fixes habituelles pas encore passées` : ''
  return `Au rythme des ${m.period.elapsed_days} premiers jours : environ ${eurosRonds(p.daily_variable)} par jour de dépenses variables pendant ${p.remaining_days} ${p.remaining_days > 1 ? 'jours' : 'jour'}${fixes}.`
}

export const RAISONS: Record<RaisonProjection, string> = {
  complete: '',
  too_early: 'Pas encore de projection : il faut au moins 5 jours de données sur le mois.',
  partial: 'Pas de projection : tes relevés ne couvrent pas le début de ce mois.',
}

export const pasDePlafond = (centimes: number) => (centimes >= 100_000 ? 5000 : centimes >= 20_000 ? 1000 : 500)
