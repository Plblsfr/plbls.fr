import type { Habitudes, Recurrent, Recurrents } from './habitudes'

const tranche = (rank: number, amount: number, count: number, days: number, jours?: [number, number]) => ({ rank, from_day: jours?.[0] ?? null, to_day: jours?.[1] ?? null, amount, count, days, per_day: days ? Math.round(amount / days) : null })

export const habitudesStats = (o: Partial<Habitudes> = {}): Habitudes => ({
  mode: 'calendar', window: '1', available: 6, current: { from: '2026-11-01', to: '2026-11-30', periods: 1 }, previous: { from: '2026-10-01', to: '2026-10-31', periods: 1 }, known_through: '2026-11-26', covered_days: 26,
  operations_from: '2026-11-01', operations_to: '2026-11-26', variable: 121_800,
  merchants: [
    { merchant_id: 41, name: 'Marché Colline', category_id: 2, amount: 26_840, count: 9, average: 2982, previous: { amount: 23_540, delta: 3300, delta_bp: 1402 }, rank_amount: 1, rank_count: 3 },
    { merchant_id: 42, name: 'Mode Atelier Nord', category_id: 4, amount: 12_900, count: 2, average: 6450, previous: { amount: 16_330, delta: -3430, delta_bp: -2100 }, rank_amount: 2, rank_count: 5 },
    { merchant_id: 43, name: 'Livréo', category_id: 3, amount: 11_840, count: 6, average: 1973, previous: { amount: 6170, delta: 5670, delta_bp: 9190 }, rank_amount: 3, rank_count: 4 },
    { merchant_id: 44, name: 'Épicerie du Canal', category_id: 2, amount: 10_600, count: 12, average: 883, previous: { amount: 10_000, delta: 600, delta_bp: 600 }, rank_amount: 4, rank_count: 2 },
    { merchant_id: 45, name: 'Boulangerie Mie Dorée', category_id: null, amount: 3820, count: 14, average: 273, previous: { amount: 0, delta: 3820, delta_bp: null }, rank_amount: 5, rank_count: 1 },
  ],
  merchants_total: 23,
  small: {
    threshold: 1500, min_repeats: 3, count: 37, amount: 18_810, yearly: 264_064, covered_days: 26, min_days: 28,
    merchants: [
      { merchant_id: 44, name: 'Épicerie du Canal', count: 12, amount: 10_600, yearly: 148_808 },
      { merchant_id: 46, name: 'Café Tamis', count: 11, amount: 4390, yearly: 61_629 },
      { merchant_id: 45, name: 'Boulangerie Mie Dorée', count: 14, amount: 3820, yearly: 53_627 },
    ],
    merchants_total: 3,
  },
  weekdays: { amount: 110_300, count: 61, buckets: [tranche(1, 14_800, 8, 4), tranche(2, 12_100, 7, 4), tranche(3, 13_900, 9, 4), tranche(4, 9400, 8, 4), tranche(5, 24_600, 11, 3), tranche(6, 29_200, 12, 3), tranche(7, 6300, 6, 4)] },
  weeks: { amount: 121_800, count: 74, buckets: [tranche(1, 26_200, 17, 7, [1, 7]), tranche(2, 30_100, 19, 7, [8, 14]), tranche(3, 34_700, 21, 7, [15, 21]), tranche(4, 30_800, 17, 5, [22, 28]), tranche(5, 0, 0, 0, [29, 31])] },
  unusual: {
    total: 2,
    operations: [
      { id: 31, date: '2026-11-24', amount: 4180, merchant_id: 43, merchant_name: 'Livréo', category_id: 3, reason: { basis: 'merchant', median: 1532, history: 9, ratio_x10: 27, excess: 2648, name: 'Livréo' } },
      { id: 32, date: '2026-11-18', amount: 9800, merchant_id: 42, merchant_name: 'Mode Atelier Nord', category_id: 4, reason: { basis: 'category', median: 3100, history: 14, ratio_x10: 32, excess: 6700, name: 'Shopping' } },
    ],
  },
  ...o,
})

export const recurrent = (o: Partial<Recurrent> = {}): Recurrent => ({
  id: '51-monthly', merchant_id: 51, name: 'Régie Belvédère', category_id: 1, group: 'charge', cadence: 'monthly', amount: 78_000, occurrences: 6, since: '2026-06-05', last_date: '2026-11-05', last_operation_id: 61,
  next_date: '2026-12-05', missed: 0, status: 'active', monthly: 78_000, yearly: 936_000, dismissed: false, alerts: [], ...o,
})

export function totaliser(items: Recurrent[], reste: Partial<Recurrents> = {}): Recurrents {
  const comptes = items.filter((x) => !x.dismissed && x.status === 'active')
  const somme = (liste: Recurrent[]) => ({ count: liste.length, monthly: liste.reduce((n, x) => n + x.monthly, 0), yearly: liste.reduce((n, x) => n + x.yearly, 0) })
  const rang = (x: Recurrent) => (x.dismissed ? 2 : x.status === 'stopped' ? 1 : 0)
  return {
    as_of: '2026-11-26', from: '2026-06-01', span_days: 179, statements: 6, detectable: ['weekly', 'monthly', 'quarterly'],
    items: [...items].sort((a, b) => rang(a) - rang(b) || b.yearly - a.yearly),
    alerts_unseen: comptes.reduce((n, x) => n + x.alerts.filter((a) => !a.seen).length, 0),
    total: somme(comptes), subscriptions: somme(comptes.filter((x) => x.group === 'subscription')), charges: somme(comptes.filter((x) => x.group === 'charge')),
    ...reste,
  }
}

export const RECURRENTS_ESSAI: Recurrent[] = [
  recurrent(),
  recurrent({ id: '52-monthly', merchant_id: 52, name: 'Voltéa Énergie', amount: 6279, monthly: 6279, yearly: 75_348, last_operation_id: 62 }),
  recurrent({ id: '53-monthly', merchant_id: 53, name: 'Salle Tonus', category_id: 9, group: 'subscription', amount: 2990, monthly: 2990, yearly: 35_880, since: '2026-06-03', last_date: '2026-11-03', next_date: '2026-12-03', last_operation_id: 63 }),
  recurrent({
    id: '54-monthly', merchant_id: 54, name: 'Streamo', category_id: 9, group: 'subscription', amount: 1399, monthly: 1399, yearly: 16_788, since: '2026-06-12', last_date: '2026-11-12', next_date: '2026-12-12', last_operation_id: 64,
    alerts: [{ kind: 'amount_changed', seen: false, date: '2026-11-12', operation_id: 64, first_date: null, old_amount: 1199, new_amount: 1399, delta: 200, delta_bp: 1668, yearly_delta: 2400 }],
  }),
  recurrent({
    id: '55-monthly', merchant_id: 55, name: 'Arcadia+', category_id: null, group: 'subscription', amount: 699, occurrences: 2, monthly: 699, yearly: 8388, since: '2026-10-08', last_date: '2026-11-08', next_date: '2026-12-08', last_operation_id: 65,
    alerts: [{ kind: 'new', seen: false, date: '2026-11-08', operation_id: 65, first_date: '2026-10-08', old_amount: null, new_amount: 699, delta: null, delta_bp: null, yearly_delta: null }],
  }),
  recurrent({ id: '56-weekly', merchant_id: 56, name: 'Panier Vert', category_id: 2, group: 'subscription', cadence: 'weekly', amount: 1500, occurrences: 12, monthly: 6500, yearly: 78_000, since: '2026-09-07', last_date: '2026-11-23', next_date: '2026-11-30', last_operation_id: 66 }),
  recurrent({ id: '57-monthly', merchant_id: 57, name: 'Presse Le Fil', category_id: 9, group: 'subscription', amount: 990, occurrences: 4, monthly: 990, yearly: 11_880, since: '2026-06-15', last_date: '2026-09-15', next_date: null, missed: 2, status: 'stopped', last_operation_id: 67 }),
  recurrent({ id: '58-monthly', merchant_id: 58, name: 'Bistrot Lune', category_id: 3, group: 'subscription', amount: 2500, occurrences: 3, monthly: 2500, yearly: 30_000, since: '2026-09-14', last_date: '2026-11-14', next_date: '2026-12-14', dismissed: true, last_operation_id: 68 }),
]

export const recurrentsStats = (o: Partial<Recurrents> = {}): Recurrents => totaliser(RECURRENTS_ESSAI, o)

export const recurrentsCourts = (): Recurrents => totaliser([], { from: '2026-11-01', span_days: 26, statements: 1, detectable: ['weekly'] })
