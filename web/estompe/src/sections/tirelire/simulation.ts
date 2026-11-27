import type { Cadence } from './habitudes'
import type { Mode } from './stats'
import type { Nature } from './tirelire'

export interface Base { method: 'average' | 'current'; periods: number; period_ids: string[]; from: string; to: string; fragile: boolean }
export interface Reference { expenses: number; income: number; savings: number; remaining: number; savings_rate_bp: number | null; budget: number | null }
export interface LigneSimulee { id: number; name: string; kind: Nature; color: number; monthly_cap: number | null; base: number; stopped: number; percent: number | null; target: number | null; saving: number; amount: number }
export interface AbonnementSimule { id: string; name: string | null; category_id: number | null; cadence: Cadence; amount: number; monthly: number; yearly: number; stopped: boolean }
export interface Effet { monthly: number; yearly: number; categories: number; subscriptions: number; expenses: number; remaining: number; savings: number; savings_rate_bp: number | null; budget_delta: number | null }
export interface Simulation { mode: Mode; period: string; basis: Base; reference: Reference; categories: LigneSimulee[]; subscriptions: AbonnementSimule[]; result: Effet }

export type Reglage = { percent: number } | { target: number }
export interface Reglages { categories: Record<number, Reglage>; arrets: string[] }
export const SANS_REGLAGE: Reglages = { categories: {}, arrets: [] }
export const PAS = 5

export function division(a: number, b: number): number {
  const q = Math.floor((2 * Math.abs(a) + b) / (2 * b))
  return a < 0 ? -q : q
}

export const pointsDeBase = (part: number, total: number) => (total > 0 ? division(part * 10_000, total) : null)

export function economie(base: number, arretes: number, percent: number | null, target: number | null): { economie: number; montant: number } {
  const ajustable = Math.max(0, base - arretes)
  const e = percent !== null ? division(ajustable * percent, 100) : target !== null ? Math.max(0, ajustable - target) : 0
  return { economie: e, montant: ajustable - e }
}

export function effet(reference: Reference, categories: number, abonnementsParMois: number, abonnementsParAn: number): Effet {
  const monthly = categories + abonnementsParMois
  const expenses = reference.expenses - monthly
  return {
    monthly, yearly: 12 * categories + abonnementsParAn, categories, subscriptions: abonnementsParMois, expenses,
    remaining: reference.remaining + monthly, savings: reference.savings + monthly, savings_rate_bp: pointsDeBase(reference.savings + monthly, reference.income),
    budget_delta: reference.budget === null ? null : expenses - reference.budget,
  }
}

export function simuler(s: Simulation, r: Reglages): Simulation {
  const subscriptions = s.subscriptions.map((a) => ({ ...a, stopped: r.arrets.includes(a.id) }))
  const arretes = new Map<number, number>()
  for (const a of subscriptions) if (a.stopped && a.category_id !== null) arretes.set(a.category_id, (arretes.get(a.category_id) ?? 0) + a.monthly)
  const categories = s.categories.map((c) => {
    const reglage = r.categories[c.id]
    const percent = reglage && 'percent' in reglage ? reglage.percent : null
    const target = reglage && 'target' in reglage ? reglage.target : null
    const stopped = Math.min(arretes.get(c.id) ?? 0, c.base)
    const e = economie(c.base, stopped, percent, target)
    return { ...c, stopped, percent, target, saving: e.economie, amount: e.montant }
  })
  const coches = subscriptions.filter((a) => a.stopped)
  return { ...s, categories, subscriptions, result: effet(s.reference, categories.reduce((n, c) => n + c.saving, 0), coches.reduce((n, a) => n + a.monthly, 0), coches.reduce((n, a) => n + a.yearly, 0)) }
}

export const reglagesVides = (r: Reglages) => !r.arrets.length && !Object.keys(r.categories).length

export function corpsSimulation(mode: Mode, r: Reglages) {
  return {
    mode,
    categories: Object.entries(r.categories).map(([id, x]) => ({ id: Number(id), ...x })).sort((a, b) => a.id - b.id),
    subscriptions: r.arrets.toSorted(),
  }
}

export const cleReglages = (mode: Mode, r: Reglages) => JSON.stringify(corpsSimulation(mode, r))

export const baisse = (c: Pick<LigneSimulee, 'base' | 'stopped' | 'saving'>) => { const ajustable = Math.max(0, c.base - c.stopped); return ajustable > 0 ? Math.round((c.saving / ajustable) * 100) : 0 }

export interface Prefill { cibles: { id: number; montant: number }[]; arrets: string[] }

export function prefillDe(requete?: string): Prefill {
  const q = new URLSearchParams(requete ?? '')
  const cibles = q.getAll('cible').flatMap((v) => { const m = /^(\d{1,15}):(\d{1,12})$/.exec(v); return m ? [{ id: Number(m[1]), montant: Number(m[2]) }] : [] })
  return { cibles, arrets: q.getAll('arret').filter((v) => /^\d{1,15}-[a-z0-9-]{1,24}$/.test(v)) }
}

export function reglagesDe(s: Simulation, p: Prefill): { reglages: Reglages; dejaSous: LigneSimulee[] } {
  const categories: Record<number, Reglage> = {}
  const dejaSous: LigneSimulee[] = []
  for (const { id, montant } of p.cibles) {
    const c = s.categories.find((x) => x.id === id)
    if (!c) continue
    if (montant >= c.base) dejaSous.push(c)
    else categories[id] = { target: montant }
  }
  return { reglages: { categories, arrets: p.arrets.filter((id) => s.subscriptions.some((a) => a.id === id)) }, dejaSous }
}

export interface Plafond { id: number; name: string; cap: number; avant: number | null }

export const plafondsDe = (s: Simulation, r: Reglages): Plafond[] => s.categories.filter((c) => r.categories[c.id] && c.saving > 0).map((c) => ({ id: c.id, name: c.name, cap: c.amount, avant: c.monthly_cap }))
