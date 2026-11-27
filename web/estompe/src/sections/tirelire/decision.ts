import { dateCourte, euros, periode, pluriel } from './formats'
import { filtresPeriode, moisDe, pourcent, unite, type Ecart, type Mode, type Moyenne, type Periode } from './stats'
import type { Plafond, Simulation } from './simulation'
import { requeteDe, type Nature } from './tirelire'

const liste = (mots: string[]) => (mots.length < 2 ? mots.join('') : `${mots.slice(0, -1).join(', ')} et ${mots.at(-1)}`)

export function messagePlafonds(p: Plafond[]): { titre: string; message: string } {
  return {
    titre: p.length > 1 ? `Fixer ${p.length} plafonds ?` : 'Fixer ce plafond ?',
    message: `${liste(p.map((x) => `${x.name} : ${euros(x.cap)} par mois (${x.avant === null ? 'aucun plafond avant' : `au lieu de ${euros(x.avant)}`})`))}. ${p.length > 1 ? 'Ils remplacent les plafonds actuels de ces catégories' : 'Il remplace le plafond actuel de cette catégorie'} ; tout reste modifiable dans Catégories et règles.`,
  }
}

const VOYELLE = /^[aeiouyàâäéèêëîïôöùûüœ]/i
export const de = (nom: string) => (VOYELLE.test(nom) ? `d’${nom}` : `de ${nom}`)

const derniers = (n: number, mode: Mode) => (mode === 'calendar' ? (n > 1 ? `tes ${n} derniers mois complets` : 'ton dernier mois complet') : n > 1 ? `tes ${n} derniers relevés` : 'ton dernier relevé')
const minuscule = (s: string) => s.charAt(0).toLowerCase() + s.slice(1)

export function phraseBase(s: Pick<Simulation, 'mode' | 'basis'>): string {
  const plage = minuscule(periode(s.basis.from, s.basis.to))
  if (s.basis.fragile) return `Base fragile : ${s.mode === 'calendar' ? 'aucun mois complet dans tes relevés' : 'aucun relevé complet'}, le calcul part de la période en cours (${plage}).`
  return `Base : ta dépense moyenne de chaque catégorie sur ${derniers(s.basis.periods, s.mode)} (${plage}).`
}

export function phraseEffet(s: Simulation): string {
  const { reference: ref, result: r } = s
  if (r.monthly <= 0) return 'Déplace un curseur, saisis une cible ou coche un abonnement pour voir ce que tu économiserais.'
  const reste = `ton reste à vivre passe de ${euros(ref.remaining)} à ${euros(r.remaining)}`
  if (ref.savings_rate_bp === null || r.savings_rate_bp === null) return `Sans revenu sur la période de base, pas de taux d’épargne ; ${reste}.`
  return `Si tu épargnes cette somme, ton taux d’épargne passe de ${pourcent(ref.savings_rate_bp, 1)} à ${pourcent(r.savings_rate_bp, 1)}. Sinon, ${reste}.`
}

export function phraseBudget(s: Simulation): string {
  const { reference: ref, result: r } = s
  if (ref.budget === null || r.budget_delta === null) return ''
  const trait = `Le trait marque ton budget de ${euros(ref.budget)}`
  const avant = ref.expenses - ref.budget
  if (r.budget_delta > 0) return `${trait} : tu resterais ${euros(r.budget_delta)} au-dessus.`
  if (avant > 0) return `${trait} : tu repasserais ${euros(-r.budget_delta)} en dessous.`
  return `${trait} : tu serais ${euros(-r.budget_delta)} en dessous.`
}

export interface Variation { category_id: number; name: string; kind: Nature; color: number; amount: number; reference_amount: number; delta: number; delta_bp: number | null }
export interface Comparaison { period_id: string | null; periods: number | null; up: Variation[]; up_total: number; down: Variation[]; down_total: number }
export type TypeLevier = 'over_cap' | 'over_average' | 'subscription_changed' | 'subscription_new' | 'merchant_up' | 'small_purchases'
export interface Levier { rank: number; kind: TypeLevier; category_id: number | null; merchant_id: number | null; recurring_id: string | null; name: string | null; amount: number; reference_amount: number; periods: number | null; count: number | null; monthly: number; yearly: number }
export interface Resume {
  mode: Mode; period: Periode; previous_id: string | null; next_id: string | null; history: 'first' | 'insufficient' | 'ok'; partial: boolean; operations: number
  expenses: { total: number; previous: Ecart | null; usual: Moyenne | null; budget: number | null; budget_delta: number | null }
  savings: { total: number; rate_bp: number | null; previous_rate_bp: number | null }
  unclassified: { amount: number; operations: number; share_bp: number | null; distorting: boolean }
  previous: Comparaison | null; previous_reason: 'none' | 'incomplete' | null; usual: Comparaison | null
  levers: Levier[]; levers_total: number
}

const avant = (r: Pick<Resume, 'mode' | 'previous'>) => (r.mode === 'calendar' && r.previous?.period_id ? moisDe(r.previous.period_id) : '')
const quAvant = (r: Pick<Resume, 'mode' | 'previous'>) => (avant(r) ? `qu’en ${avant(r)}` : 'que sur le relevé précédent')
const signe = (delta: number) => `${delta > 0 ? '+' : '−'}${euros(Math.abs(delta))}`
const signePourcent = (bp: number) => `${bp > 0 ? '+' : '−'}${pourcent(Math.abs(bp))}`

export const titreResume = (r: Pick<Resume, 'mode' | 'period'>) => (r.mode === 'calendar' ? `${moisDe(r.period.id).replace(/^./, (c) => c.toUpperCase())} en bref` : 'Ce relevé en bref')

export function phraseDepenses(r: Resume): string {
  const ou = r.mode === 'calendar' ? `en ${moisDe(r.period.id)}` : `sur le relevé du ${dateCourte(r.period.to)}`
  const debut = `${r.period.open ? `Au ${dateCourte(r.period.known_through)}, tu` : 'Tu'} as dépensé ${euros(r.expenses.total)} ${ou}`
  const suites: string[] = []
  const e = r.expenses.previous
  if (e && !(r.partial && e.delta <= 0)) {
    if (e.delta === 0) suites.push(`autant ${quAvant(r)}`)
    else suites.push(`${euros(Math.abs(e.delta))} ${e.delta > 0 ? 'de plus' : 'de moins'} ${quAvant(r)}${e.delta_bp === null ? '' : ` (${signePourcent(e.delta_bp)})`}`)
  }
  const b = r.expenses.budget_delta
  if (b !== null && b > 0) suites.push(`${euros(b)} au-dessus de ton budget`)
  else if (b !== null && r.partial) suites.push(`encore ${euros(-b)} sous ton budget`)
  else if (b !== null) suites.push(`${euros(-b)} sous ton budget`)
  return suites.length ? `${debut}, soit ${suites.join(' et ')}.` : `${debut}.`
}

export function referenceDe(r: Pick<Resume, 'mode' | 'previous'>, c: Comparaison): string {
  if (c.periods === null) return avant(r) ? `Par rapport à ${avant(r)}` : 'Par rapport au relevé précédent'
  if (r.mode === 'calendar') return c.periods > 1 ? `Par rapport à ta moyenne des ${c.periods} derniers mois complets` : 'Par rapport à ton dernier mois complet'
  return c.periods > 1 ? `Par rapport à ta moyenne des ${c.periods} derniers relevés` : 'Par rapport au relevé d’avant'
}

export function variations(c: Comparaison, sens: 'up' | 'down'): string {
  const [cites, total] = sens === 'up' ? [c.up, c.up_total] : [c.down, c.down_total]
  if (!cites.length) return ''
  const mots = cites.map((v) => `${v.name} (${signe(v.delta)})`)
  const reste = total - cites.length
  return liste(reste > 0 ? [...mots, reste > 1 ? `${reste} autres catégories` : '1 autre catégorie'] : mots)
}

export function phraseVariations(r: Pick<Resume, 'mode' | 'previous'>, c: Comparaison, sens: 'up' | 'down'): string {
  const total = sens === 'up' ? c.up_total : c.down_total
  if (!total) return ''
  return `${referenceDe(r, c)}, ${total > 1 ? `${total} catégories sont` : '1 catégorie est'} ${sens === 'up' ? 'en hausse' : 'en baisse'} : ${variations(c, sens)}.`
}

export function phraseEpargne(r: Resume): string {
  if (r.savings.rate_bp === null) return ''
  const contre = r.savings.previous_rate_bp === null ? '' : `, contre ${pourcent(r.savings.previous_rate_bp, 1)} ${avant(r) ? `en ${avant(r)}` : 'sur le relevé précédent'}`
  return `Taux d’épargne de ${pourcent(r.savings.rate_bp, 1)}${contre}.`
}

export function phraseAClasser(r: Resume): string {
  const u = r.unclassified
  if (!u.distorting) return ''
  const part = u.share_bp === null ? '' : `, ${pourcent(u.share_bp)} des dépenses`
  return `${pluriel(u.operations, 'opération à classer', 'opérations à classer')} (${euros(u.amount)}${part}) ${u.operations > 1 ? 'faussent' : 'fausse'} ce résumé : classe-${u.operations > 1 ? 'les' : 'la'} pour le fiabiliser.`
}

export function phraseEtat(r: Resume): string {
  const un = r.mode === 'calendar' ? 'un deuxième mois' : 'un deuxième relevé'
  if (r.history === 'first') return `C’est ta première période : rien à comparer pour l’instant. Les écarts apparaîtront avec ${un}.`
  if (r.history === 'insufficient') return r.previous_reason === 'incomplete' ? 'Données insuffisantes pour comparer : le mois précédent n’est couvert qu’en partie par tes relevés, et aucun mois complet ne permet encore de calculer une moyenne.' : 'Données insuffisantes pour comparer : aucun relevé ne couvre le mois précédent, et aucun mois complet ne permet encore de calculer une moyenne.'
  const notes: string[] = []
  if (!r.previous && r.previous_reason === 'incomplete') notes.push('Pas de comparaison avec le mois précédent : tes relevés ne le couvrent qu’en partie.')
  else if (!r.previous) notes.push('Pas de comparaison avec le mois précédent : aucun relevé ne le couvre.')
  if (r.partial) notes.push(r.period.open ? 'La période est en cours : les baisses ne sont pas citées tant qu’elle n’est pas terminée.' : 'Tes relevés ne couvrent cette période qu’en partie : les baisses ne sont pas citées.')
  const rien = [r.previous, r.usual].every((c) => !c || (!c.up_total && !c.down_total))
  if (rien) notes.push(`Rien de notable : aucune catégorie ne s’écarte d’au moins 10 € et 10 % ${r.partial ? 'au-dessus ' : ''}de ses repères.`)
  return notes.join(' ')
}

const sur = (n: number, mode: Mode) => `${n} ${unite(mode, n)}`

export function titreLevier(l: Levier): string {
  if (l.kind === 'small_purchases') return 'Petits achats répétés'
  return l.name ?? (l.kind === 'over_cap' || l.kind === 'over_average' ? 'Catégorie' : 'Commerçant sans nom')
}

export function detailLevier(l: Levier, r: Pick<Resume, 'mode' | 'previous'>): string {
  switch (l.kind) {
    case 'over_cap': return `revenir au plafond de ${euros(l.reference_amount)} (dépensé : ${euros(l.amount)})`
    case 'over_average': return `revenir à ta moyenne de ${euros(l.reference_amount)} sur ${sur(l.periods ?? 1, r.mode)} (dépensé : ${euros(l.amount)})`
    case 'subscription_changed': return `revenir à l’ancien tarif de ${euros(l.reference_amount)} (passé à ${euros(l.amount)})`
    case 'subscription_new': return `nouvel abonnement (${euros(l.amount)} à chaque échéance) : en as-tu besoin ?`
    case 'merchant_up': return `revenir au niveau ${avant(r) ? de(avant(r)) : 'du relevé précédent'} : ${euros(l.reference_amount)} au lieu de ${euros(l.amount)}`
    case 'small_purchases': return `en éviter un sur deux : ${pluriel(l.count ?? 0, 'petit achat', 'petits achats')} sur ${sur(l.periods ?? 1, r.mode)}${l.name ? `, surtout chez ${l.name}` : ''}`
  }
}

export const gainLevier = (l: Levier) => `${euros(l.monthly)} par mois, ${euros(l.yearly)} par an`

export const phraseLevier = (l: Levier, r: Pick<Resume, 'mode' | 'previous'>) => `${titreLevier(l)} : ${detailLevier(l, r)}. Gain estimé : ${gainLevier(l)}.`

export const actionLevier = (l: Levier) => (l.kind === 'over_cap' || l.kind === 'over_average' || l.kind === 'subscription_new' ? 'Simuler' : l.kind === 'subscription_changed' ? 'Voir l’abonnement' : l.kind === 'small_purchases' ? 'Voir les habitudes' : 'Voir les opérations')

export interface Lien { section: string; id?: string; requete?: string }

export function lienLevier(l: Levier, r: Pick<Resume, 'mode' | 'period'>): Lien {
  if ((l.kind === 'over_cap' || l.kind === 'over_average') && l.category_id !== null) return { section: 'decision', requete: `cible=${l.category_id}:${l.reference_amount}` }
  if (l.kind === 'subscription_new' && l.recurring_id) return { section: 'decision', requete: `arret=${l.recurring_id}` }
  if (l.kind === 'subscription_changed') return { section: 'habitudes', id: 'abonnements' }
  if (l.kind === 'merchant_up' && l.merchant_id !== null) return { section: 'operations', requete: requeteDe(filtresPeriode(r.period, r.mode, { commercant: l.merchant_id, sens: 'debit' })) }
  return { section: 'habitudes' }
}

export const lienDecision = (periodeId?: string): Lien => ({ section: 'decision', requete: periodeId ? `periode=${periodeId}` : '' })
