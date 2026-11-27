import type { Resume } from './decision'
import { effet, type LigneSimulee, type Simulation } from './simulation'
import { periodeStats } from './essais'

const ligne = (id: number, name: string, base: number, o: Partial<LigneSimulee> = {}): LigneSimulee => ({ id, name, kind: 'variable', color: id - 1, monthly_cap: null, base, stopped: 0, percent: null, target: null, saving: 0, amount: base, ...o })

export const simulationStats = (o: Partial<Simulation> = {}): Simulation => {
  const reference = { expenses: 204_400, income: 270_000, savings: 30_000, remaining: 35_600, savings_rate_bp: 1111, budget: 205_000, ...o.reference }
  return {
    mode: 'calendar', period: '2026-11',
    basis: { method: 'average', periods: 3, period_ids: ['2026-08', '2026-09', '2026-10'], from: '2026-08-01', to: '2026-10-31', fragile: false },
    categories: [
      ligne(1, 'Logement', 78_000, { kind: 'fixe', monthly_cap: 78_000 }),
      ligne(2, 'Courses', 41_260, { monthly_cap: 38_000 }),
      ligne(3, 'Restaurants', 23_680, { monthly_cap: 15_000 }),
      ligne(4, 'Shopping', 18_735),
      ligne(5, 'Loisirs et sorties', 16_490, { monthly_cap: 12_000 }),
      ligne(6, 'Transports', 14_800),
      ligne(9, 'Abonnements', 11_847, { kind: 'fixe', monthly_cap: 11_000 }),
      ligne(7, 'Santé', 0),
    ],
    subscriptions: [
      { id: '56-weekly', name: 'Panier Vert', category_id: 2, cadence: 'weekly', amount: 1500, monthly: 6500, yearly: 78_000, stopped: false },
      { id: '53-monthly', name: 'Salle Tonus', category_id: 9, cadence: 'monthly', amount: 2990, monthly: 2990, yearly: 35_880, stopped: false },
      { id: '54-monthly', name: 'Streamo', category_id: 9, cadence: 'monthly', amount: 1399, monthly: 1399, yearly: 16_788, stopped: false },
    ],
    result: effet(reference, 0, 0, 0),
    ...o,
    reference,
  }
}

export const resumeStats = (o: Partial<Resume> = {}): Resume => ({
  mode: 'calendar', period: periodeStats(), previous_id: '2026-10', next_id: null, history: 'ok', partial: true, operations: 84,
  expenses: { total: 221_340, previous: { amount: 202_700, delta: 18_640, delta_bp: 920 }, usual: { amount: 204_400, periods: 3, delta: 16_940, delta_bp: 829 }, budget: 205_000, budget_delta: 16_340 },
  savings: { total: 25_000, rate_bp: 880, previous_rate_bp: 1109 },
  unclassified: { amount: 2800, operations: 7, share_bp: 127, distorting: false },
  previous: {
    period_id: '2026-10', periods: null, up_total: 2, down_total: 0, down: [],
    up: [
      { category_id: 3, name: 'Restaurants', kind: 'variable', color: 2, amount: 23_680, reference_amount: 14_800, delta: 8880, delta_bp: 6000 },
      { category_id: 2, name: 'Courses', kind: 'variable', color: 1, amount: 41_260, reference_amount: 37_100, delta: 4160, delta_bp: 1121 },
    ],
  },
  previous_reason: null,
  usual: { period_id: null, periods: 3, up_total: 1, down_total: 0, down: [], up: [{ category_id: 3, name: 'Restaurants', kind: 'variable', color: 2, amount: 23_680, reference_amount: 17_300, delta: 6380, delta_bp: 3688 }] },
  levers: [
    { rank: 1, kind: 'over_cap', category_id: 3, merchant_id: null, recurring_id: null, name: 'Restaurants', amount: 23_680, reference_amount: 15_000, periods: null, count: null, monthly: 8680, yearly: 104_160 },
    { rank: 2, kind: 'small_purchases', category_id: null, merchant_id: 44, recurring_id: null, name: 'Épicerie du Canal', amount: 24_660, reference_amount: 100_000, periods: 3, count: 96, monthly: 4167, yearly: 50_000 },
    { rank: 3, kind: 'over_cap', category_id: 2, merchant_id: null, recurring_id: null, name: 'Courses', amount: 41_260, reference_amount: 38_000, periods: null, count: null, monthly: 3260, yearly: 39_120 },
  ],
  levers_total: 5,
  ...o,
})

export const resumePremier = (o: Partial<Resume> = {}): Resume => resumeStats({
  previous_id: null, history: 'first', expenses: { total: 221_340, previous: null, usual: null, budget: null, budget_delta: null }, savings: { total: 25_000, rate_bp: 880, previous_rate_bp: null },
  previous: null, previous_reason: 'none', usual: null, levers: [], levers_total: 0, ...o,
})
