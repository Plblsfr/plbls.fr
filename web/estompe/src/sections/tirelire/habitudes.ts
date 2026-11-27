import { dateCourte, euros, pluriel } from './formats'
import { eurosRonds, unite, type Ecart, type Mode } from './stats'
import { requeteDe, filtres } from './tirelire'

export type FenetreHabitudes = '1' | '3' | '6' | '12' | 'all'
export const FENETRES_HABITUDES: FenetreHabitudes[] = ['1', '3', '6', '12', 'all']
export const CLE_FENETRE_HABITUDES = 'tirelire.habitudes'
export const lireFenetreHabitudes = (v: string | null | undefined): FenetreHabitudes => (FENETRES_HABITUDES.includes(v as FenetreHabitudes) ? (v as FenetreHabitudes) : '3')
export const nomFenetreHabitudes = (f: FenetreHabitudes, mode: Mode) => (f === 'all' ? 'Tout' : f === '1' ? (mode === 'calendar' ? 'Ce mois' : 'Ce relevé') : `${f} ${unite(mode, Number(f))}`)

export interface Plage { from: string; to: string; periods: number }
export interface LigneCommercant { merchant_id: number; name: string | null; category_id: number | null; amount: number; count: number; average: number | null; previous: Ecart | null; rank_amount: number; rank_count: number }
export interface PetitCommercant { merchant_id: number; name: string | null; count: number; amount: number; yearly: number | null }
export interface PetitsAchats { threshold: number; min_repeats: number; count: number; amount: number; yearly: number | null; covered_days: number; min_days: number; merchants: PetitCommercant[]; merchants_total: number }
export interface Tranche { rank: number; from_day: number | null; to_day: number | null; amount: number; count: number; days: number; per_day: number | null }
export interface Repartition { amount: number; count: number; buckets: Tranche[] }
export interface Raison { basis: 'merchant' | 'category'; median: number; history: number; ratio_x10: number | null; excess: number; name: string | null }
export interface Inhabituelle { id: number; date: string; amount: number; merchant_id: number | null; merchant_name: string | null; category_id: number | null; reason: Raison }
export interface Habitudes {
  mode: Mode; window: FenetreHabitudes; available: number; current: Plage | null; previous: Plage | null; known_through: string | null; covered_days: number
  operations_from: string | null; operations_to: string | null; variable: number
  merchants: LigneCommercant[]; merchants_total: number
  small: PetitsAchats
  weekdays: Repartition; weeks: Repartition
  unusual: { total: number; operations: Inhabituelle[] }
}

export type Cadence = 'weekly' | 'monthly' | 'quarterly' | 'yearly'
export type Groupe = 'subscription' | 'charge'
export interface Alerte { kind: 'new' | 'amount_changed'; seen: boolean; date: string; operation_id: number; first_date: string | null; old_amount: number | null; new_amount: number; delta: number | null; delta_bp: number | null; yearly_delta: number | null }
export interface Recurrent {
  id: string; merchant_id: number; name: string | null; category_id: number | null; group: Groupe; cadence: Cadence; amount: number; occurrences: number
  since: string; last_date: string; last_operation_id: number; next_date: string | null; missed: number; status: 'active' | 'stopped'; monthly: number; yearly: number; dismissed: boolean; alerts: Alerte[]
}
export interface TotauxRecurrents { count: number; monthly: number; yearly: number }
export interface Recurrents { as_of: string | null; from: string | null; span_days: number; statements: number; detectable: Cadence[]; items: Recurrent[]; alerts_unseen: number; total: TotauxRecurrents; subscriptions: TotauxRecurrents; charges: TotauxRecurrents }

export const TOP = 10
export type Tri = 'montant' | 'nombre'

export function classes(h: Habitudes, tri: Tri): LigneCommercant[] {
  const rang = (m: LigneCommercant) => (tri === 'montant' ? m.rank_amount : m.rank_count)
  return h.merchants.filter((m) => rang(m) <= TOP).sort((a, b) => rang(a) - rang(b))
}

export function lienOperations(h: Pick<Habitudes, 'mode' | 'operations_from' | 'operations_to'>, commercant?: number): string {
  return requeteDe(filtres({ du: h.operations_from ?? '', au: h.operations_to ?? '', parAchat: h.mode === 'calendar', commercant: commercant ?? null, sens: commercant === undefined ? null : 'debit' }))
}

export const JOURS = ['lundi', 'mardi', 'mercredi', 'jeudi', 'vendredi', 'samedi', 'dimanche']
export const JOURS_COURTS = ['lun.', 'mar.', 'mer.', 'jeu.', 'ven.', 'sam.', 'dim.']
const liste = (mots: string[]) => (mots.length < 2 ? mots.join('') : `${mots.slice(0, -1).join(', ')} et ${mots.at(-1)}`)
const majuscule = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

export function forts(r: Repartition): number[] {
  const tries = r.buckets.filter((t) => t.amount > 0).sort((a, b) => b.amount - a.amount)
  if (tries.length < 3) return tries.slice(0, 1).map((t) => t.rank)
  return tries.slice(0, 2).map((t) => t.rank).sort((a, b) => a - b)
}

export function phraseJours(r: Repartition): string {
  const hauts = forts(r)
  if (!hauts.length || r.amount <= 0) return ''
  const somme = r.buckets.filter((t) => hauts.includes(t.rank)).reduce((n, t) => n + t.amount, 0)
  const part = Math.round((somme / r.amount) * 100)
  return `${majuscule(liste(hauts.map((k) => JOURS[k - 1])))} ${hauts.length > 1 ? 'concentrent' : 'concentre'} ${part} % de tes paiements par carte (${eurosRonds(somme)} sur ${eurosRonds(r.amount)}).`
}

export const nomSemaine = (t: Tranche) => `S${t.rank} · ${t.from_day}–${t.to_day}`
export const nomSemaineLong = (t: Tranche) => `du ${t.from_day === 1 ? '1er' : t.from_day} au ${t.to_day}`

export function phraseSemaines(r: Repartition): string {
  const pleines = r.buckets.filter((t) => t.days > 0 && t.per_day !== null)
  if (r.amount <= 0 || pleines.length < 2) return ''
  const haute = pleines.toSorted((a, b) => (b.per_day ?? 0) - (a.per_day ?? 0))[0]
  return `C’est ${nomSemaineLong(haute)} que tu dépenses le plus par jour : ${euros(haute.per_day ?? 0)} en moyenne.`
}

export const fois = (x10: number) => `${Math.trunc(x10 / 10)},${x10 % 10} ×`

export function raisonDe(r: Raison): string {
  const multiple = r.ratio_x10 === null ? `${euros(r.excess)} au-dessus de` : `${fois(r.ratio_x10)}`
  if (r.basis === 'merchant') return `${multiple} ton panier habituel chez ${r.name ?? 'ce commerçant'} (${euros(r.median)})`
  return `${multiple} ton achat habituel en ${r.name ?? 'cette catégorie'} (${euros(r.median)})`
}

export const methodeRaison = (r: Raison) => `Comparée à la médiane de ${pluriel(r.history, 'autre achat', 'autres achats')} ${r.basis === 'merchant' ? 'chez ce commerçant' : 'de cette catégorie'}.`

export const CADENCES: Record<Cadence, string> = { weekly: 'hebdomadaire', monthly: 'mensuel', quarterly: 'trimestriel', yearly: 'annuel' }
export const PAR: Record<Cadence, string> = { weekly: 'par semaine', monthly: 'par mois', quarterly: 'par trimestre', yearly: 'par an' }
const MOIS = new Intl.DateTimeFormat('fr-FR', { month: 'long', timeZone: 'UTC' })
const SEMAINE = new Intl.DateTimeFormat('fr-FR', { weekday: 'long', timeZone: 'UTC' })
const date = (jour: string) => new Date(`${jour}T00:00:00Z`)
const quantieme = (jour: string) => (Number(jour.slice(8, 10)) === 1 ? '1er' : String(Number(jour.slice(8, 10))))

export function cadenceDe(r: Pick<Recurrent, 'cadence' | 'last_date'>): string {
  if (r.cadence === 'weekly') return `hebdomadaire, le ${SEMAINE.format(date(r.last_date))}`
  if (r.cadence === 'yearly') return `annuel, en ${MOIS.format(date(r.last_date))}`
  return `${CADENCES[r.cadence]}, vers le ${quantieme(r.last_date)}`
}

export const actifs = (r: Recurrents) => r.items.filter((x) => !x.dismissed && x.status === 'active')
export const arretes = (r: Recurrents) => r.items.filter((x) => !x.dismissed && x.status === 'stopped')
export const ecartes = (r: Recurrents) => r.items.filter((x) => x.dismissed)

export interface AlerteDe { recurrent: Recurrent; alerte: Alerte }
export function alertes(r: Recurrents): AlerteDe[] {
  return actifs(r).flatMap((x) => x.alerts.filter((a) => !a.seen).map((a) => ({ recurrent: x, alerte: a }))).sort((a, b) => b.alerte.date.localeCompare(a.alerte.date))
}

export function phraseAlerte({ recurrent, alerte }: AlerteDe): string {
  if (alerte.kind === 'amount_changed') {
    const signe = (alerte.delta ?? 0) > 0 ? '+' : '−'
    return `Depuis le ${dateCourte(alerte.date)} : ${signe}${euros(Math.abs(alerte.delta ?? 0))} ${PAR[recurrent.cadence]}, soit ${signe}${euros(Math.abs(alerte.yearly_delta ?? 0))} par an.`
  }
  return `${pluriel(recurrent.occurrences, 'échéance identique', 'échéances identiques')} depuis le ${dateCourte(alerte.first_date ?? recurrent.since)}, la dernière le ${dateCourte(alerte.date)} : ${euros(recurrent.yearly)} par an.`
}

const BESOINS: Record<Cadence, string> = {
  weekly: '4 semaines pour un paiement hebdomadaire',
  monthly: '2 relevés qui se suivent pour un prélèvement mensuel (3 pour un paiement par carte)',
  quarterly: '2 trimestres pour un prélèvement trimestriel',
  yearly: '13 mois pour un abonnement annuel',
}

export function manque(r: Pick<Recurrents, 'detectable'>): string {
  const absentes = (Object.keys(BESOINS) as Cadence[]).filter((c) => !r.detectable.includes(c))
  return absentes.length ? `Il faut ${liste(absentes.map((c) => BESOINS[c]))}.` : ''
}

export function couverture(r: Pick<Recurrents, 'statements' | 'span_days'>): string {
  return `${pluriel(r.statements, 'relevé importé', 'relevés importés')} · ${pluriel(r.span_days, 'jour couvert', 'jours couverts')}`
}
