import { dateCourte, nombre } from './formats'
import { eurosRonds, eurosRondsSignes, pourcentSigne, unite, type Ecart, type Mode } from './stats'
import type { Nature } from './tirelire'

export type Fenetre = '6' | '12' | '24' | 'all'
export const FENETRES: Fenetre[] = ['6', '12', '24', 'all']
export const CLE_FENETRE = 'tirelire.fenetre'
export const lireFenetre = (v: string | null | undefined): Fenetre => (FENETRES.includes(v as Fenetre) ? (v as Fenetre) : '12')
export const nomFenetre = (f: Fenetre, mode: Mode) => (f === 'all' ? 'Tout' : `${f} ${unite(mode, Number(f))}`)

export type Direction = 'up' | 'down' | 'stable' | 'insufficient'
export interface Tendance { direction: Direction; periods: number; average: number | null; delta: number | null; delta_bp: number | null; values: number[] }
export interface PeriodeHistorique { id: string; from: string; to: string; complete: boolean; open: boolean; expenses: number }
export interface LigneHistorique { id: number; name: string; kind: Nature; color: number; archived: boolean; monthly_cap: number | null; amounts: number[]; total: number; average: number | null; over_cap: number | null; trend: Tendance }
export interface Historique {
  mode: Mode; window: Fenetre; available: number; complete: number; periods: PeriodeHistorique[]; trend_period_ids: string[]
  categories: LigneHistorique[]
  unclassified: { amounts: number[]; total: number; average: number | null }
  total: { amounts: number[]; total: number; average: number | null; budget: number | null; over_budget: number | null; trend: Tendance }
  chart: { category_ids: number[]; others: number[]; others_categories: number }
}

export interface Jour { date: string; amount: number }
export interface Segment { from: string; to: string; values: number[] }
export interface Trou { from: string; to: string; days: number }
export interface SoldeCompte { account_id: number; days: number; reconciled: boolean; segments: Segment[]; gaps: Trou[]; first: Jour | null; last: Jour | null; min: Jour | null; max: Jour | null }
export interface Soldes { mode: Mode; window: Fenetre; from: string | null; to: string | null; accounts: SoldeCompte[] }

export interface Cumul { current: number; previous: number; delta: number; delta_bp: number | null }
export interface Comparaison { month: number; month_previous: Ecart | null; ytd: number; ytd_compared: Cumul | null }
export interface LigneAnnee extends Comparaison { id: number; name: string; kind: Nature; color: number }
export interface MoisAnnee { month: number; current: number | null; current_complete: boolean | null; previous: number | null; previous_complete: boolean | null }
export interface Annees { mode: Mode; period: string; year: number; month: number; previous_year: number; has_previous: boolean; compared_months: number[]; partial: boolean; months: MoisAnnee[]; total: Comparaison; categories: LigneAnnee[]; unclassified: Comparaison }

export interface Evolution { historique: Historique; soldes: Soldes; annees: Annees | null }

const UTC = { timeZone: 'UTC' } as const
const MOIS_COURT = new Intl.DateTimeFormat('fr-FR', { month: 'short', ...UTC })
const MOIS_LONG = new Intl.DateTimeFormat('fr-FR', { month: 'long', ...UTC })
const INITIALE = new Intl.DateTimeFormat('fr-FR', { month: 'narrow', ...UTC })
const date = (jour: string) => new Date(`${jour}T00:00:00Z`)
const JOUR_MS = 86_400_000

export const moisCourt = (m: number) => MOIS_COURT.format(new Date(Date.UTC(2026, m - 1, 1)))
export const moisEntier = (m: number) => MOIS_LONG.format(new Date(Date.UTC(2026, m - 1, 1)))
export const initiale = (m: number) => INITIALE.format(new Date(Date.UTC(2026, m - 1, 1))).toUpperCase()

export const ecartJours = (du: string, au: string) => Math.round((date(au).getTime() - date(du).getTime()) / JOUR_MS)
export const plusJours = (jour: string, n: number) => new Date(date(jour).getTime() + n * JOUR_MS).toISOString().slice(0, 10)

export function colonne(p: Pick<PeriodeHistorique, 'id' | 'from' | 'to'>, mode: Mode): string {
  return mode === 'calendar' ? MOIS_COURT.format(date(p.from)) : `${p.to.slice(8, 10)}/${p.to.slice(5, 7)}`
}

export const anneeDe = (p: Pick<PeriodeHistorique, 'from' | 'to'>, mode: Mode) => (mode === 'calendar' ? p.from : p.to).slice(0, 4)

export function annees(periodes: Pick<PeriodeHistorique, 'from' | 'to'>[], mode: Mode): string[] {
  return periodes.map((p, i) => (i === 0 || anneeDe(p, mode) !== anneeDe(periodes[i - 1], mode) ? anneeDe(p, mode) : ''))
}

const borne = (p: Pick<PeriodeHistorique, 'id' | 'from' | 'to'>, mode: Mode) => (mode === 'calendar' ? `${colonne(p, mode)} ${anneeDe(p, mode)}` : dateCourte(p.to))
export const nomColonne = (p: Pick<PeriodeHistorique, 'id' | 'from' | 'to'>, mode: Mode) => (mode === 'calendar' ? `${MOIS_LONG.format(date(p.from))} ${p.from.slice(0, 4)}` : `relevé du ${dateCourte(p.to)}`)

export const etendue = (periodes: Pick<PeriodeHistorique, 'id' | 'from' | 'to'>[], mode: Mode) => (periodes.length ? (periodes.length === 1 ? nomColonne(periodes[0], mode) : `${borne(periodes[0], mode)} → ${borne(periodes.at(-1)!, mode)}`) : '')

export const arrondi = (centimes: number) => `${centimes < 0 ? '−' : ''}${nombre(Math.round(Math.abs(centimes) / 100))}`

export const MOTS_DIRECTION: Record<Direction, string> = { up: 'hausse', down: 'baisse', stable: 'stable', insufficient: 'pas assez de données' }
export const FLECHES_DIRECTION: Record<Direction, string> = { up: '▲', down: '▼', stable: '=', insufficient: '' }

export function etiquetteTendance(t: Tendance): string {
  if (t.direction === 'insufficient') return MOTS_DIRECTION.insufficient
  const chiffre = t.delta_bp === null ? (t.delta ? eurosRonds(t.delta) : '') : pourcentSigne(t.delta_bp).replace(/^=\s*/, '')
  return [MOTS_DIRECTION[t.direction], chiffre].filter(Boolean).join(' ')
}

export function detailTendance(t: Tendance, mode: Mode): string {
  if (t.direction === 'insufficient' || t.delta === null || t.average === null) return `${t.periods} ${unite(mode, t.periods)} ${t.periods > 1 ? 'complets' : 'complet'} sur 3 nécessaires`
  return `${t.delta === 0 ? eurosRonds(0) : eurosRondsSignes(t.delta)} en ${t.periods} ${unite(mode, t.periods)} · moy. ${eurosRonds(t.average)}`
}

export function compte(lignes: { trend: Tendance }[]): Record<Direction, number> {
  const res: Record<Direction, number> = { up: 0, down: 0, stable: 0, insufficient: 0 }
  for (const l of lignes) res[l.trend.direction] += 1
  return res
}

export interface Tranche { cle: string; nom: string; teinte: number | 'autres' | 'classer'; montant: number }

export function tranches(h: Historique, k: number, teinte: (c: { color: number }) => number): Tranche[] {
  const res: Tranche[] = h.chart.category_ids.flatMap((id) => {
    const c = h.categories.find((x) => x.id === id)
    return c ? [{ cle: String(id), nom: c.name, teinte: teinte(c), montant: c.amounts[k] }] : []
  })
  if (h.chart.others_categories > 0) res.push({ cle: 'autres', nom: `Autres (${h.chart.others_categories})`, teinte: 'autres', montant: h.chart.others[k] })
  if (h.unclassified.amounts.some((m) => m !== 0)) res.push({ cle: 'classer', nom: 'À classer', teinte: 'classer', montant: h.unclassified.amounts[k] })
  return res
}

export interface Echelle { min: number; max: number; valeurs: number[] }

export function echelleSolde(min: number, max: number, cible = 3): Echelle {
  const bas = Math.min(0, min)
  const haut = Math.max(0, max, bas + 100)
  const brut = (haut - bas) / cible
  const puissance = 10 ** Math.floor(Math.log10(brut))
  const pas = [1, 2, 2.5, 5, 10].map((k) => k * puissance).find((k) => k >= brut) ?? 10 * puissance
  const debut = Math.floor(bas / pas) * pas
  const fin = Math.ceil(haut / pas) * pas
  const valeurs: number[] = []
  for (let v = debut; v <= fin + 0.5; v += pas) valeurs.push(Math.round(v))
  return { min: debut, max: fin, valeurs }
}

export function soldeLe(c: SoldeCompte, jour: string): number | null {
  const s = c.segments.find((x) => x.from <= jour && jour <= x.to)
  return s ? s.values[ecartJours(s.from, jour)] : null
}

export interface Palier { date: string; amount: number | null; fin?: string }

export function paliers(c: SoldeCompte): Palier[] {
  const res: Palier[] = []
  for (const s of c.segments) {
    s.values.forEach((v, i) => { if (i === 0 || v !== s.values[i - 1]) res.push({ date: plusJours(s.from, i), amount: v }) })
  }
  for (const t of c.gaps) res.push({ date: t.from, amount: null, fin: t.to })
  return res.sort((a, b) => a.date.localeCompare(b.date))
}

export function debutsDeMois(du: string, au: string, maximum: number): string[] {
  const res: string[] = []
  let [a, m] = [Number(du.slice(0, 4)), Number(du.slice(5, 7))]
  for (;;) {
    const j = `${a}-${String(m).padStart(2, '0')}-01`
    if (j > au) break
    if (j >= du) res.push(j)
    m += 1
    if (m > 12) { m = 1; a += 1 }
  }
  const pas = Math.max(1, Math.ceil(res.length / Math.max(1, maximum)))
  return res.filter((_, i) => (res.length - 1 - i) % pas === 0)
}

export const etiquetteMois = (jour: string, avecAnnee: boolean) => `${MOIS_COURT.format(date(jour))}${avecAnnee ? ` ${jour.slice(0, 4)}` : ''}`

export function suites<T>(valeurs: (T | null)[]): { debut: number; valeurs: T[] }[] {
  const res: { debut: number; valeurs: T[] }[] = []
  valeurs.forEach((v, i) => {
    if (v === null) return
    const derniere = res.at(-1)
    if (derniere && derniere.debut + derniere.valeurs.length === i) derniere.valeurs.push(v)
    else res.push({ debut: i, valeurs: [v] })
  })
  return res
}
