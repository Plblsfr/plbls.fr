import type { Annees, Historique, LigneHistorique, Soldes, Tendance } from './evolution'
import type { DetailCategorie, LigneCategorie, Mois, Periode } from './stats'
import type { Categorie, ClientTirelire, Commercant, FileAClasser, Groupe, Operation, PageOperations, Regle, Releve, ResultatImport } from './tirelire'

export const releve = (o: Partial<Releve> = {}): Releve => ({
  id: 11, account_id: 1, number: 11, year: 2026, from: '2026-10-31', to: '2026-11-26',
  old_balance: 152_340, new_balance: 190_000, total_debits: 246_340, total_credits: 284_000,
  operations: 84, pages: 3, sha256: 'a'.repeat(64),
  checks: { total_debits: { expected: 246_340, read: 246_340 }, total_credits: { expected: 284_000, read: 284_000 }, new_balance: { expected: 190_000, read: 190_000 }, operations: 84, unrecognized_lines: 0 },
  imported_at: '2026-11-26T09:30:00Z',
  ...o,
})

export const operation = (o: Partial<Operation> = {}): Operation => ({
  id: 1, statement_id: 11, account_id: 1, date: '2026-11-26', value_date: '2026-11-26', purchase_date: null,
  section: 'PAIEMENTS PAR CARTE', type: 'carte', label: 'CARTE 25/11 MARCHE COLLINE VILLENEUVE', details: [], merchant: 'MARCHE COLLINE VILLENEUVE',
  amount: 3120, direction: 'debit', merchant_id: null, merchant_name: null, category_id: null, category_source: null, rule_id: null,
  ...o,
})

export const OPERATIONS: Operation[] = [
  operation({ id: 1, date: '2026-11-26', purchase_date: '2026-11-25' }),
  operation({ id: 2, date: '2026-11-26', label: 'CARTE 26/11 CAFE TAMIS VILLENEUVE', merchant: 'CAFE TAMIS VILLENEUVE', amount: 390, purchase_date: '2026-11-26' }),
  operation({ id: 3, date: '2026-11-24', value_date: '2026-11-24', purchase_date: '2026-11-22', label: 'CARTE 22/11 LIVREO*CMD PARIS', merchant: 'LIVREO*CMD PARIS', details: ['LIVREO*CMD 88213', 'REF 0KX72A91'], amount: 4180 }),
  operation({ id: 4, date: '2026-11-24', section: 'VIREMENTS EMIS ET PRELEVEMENTS', type: 'virement', label: 'VIR VERS LIVRET', merchant: null, amount: 25_000 }),
  operation({ id: 5, date: '2026-11-23', section: 'VIREMENTS RECUS', type: 'virement', label: 'VIR ATELIER MERIDIEN SALAIRE', details: ['SALAIRE NOVEMBRE'], merchant: null, amount: 264_000, direction: 'credit' }),
]

export const page = (operations: Operation[] = OPERATIONS, total = operations.length, offset = 0): PageOperations => ({ operations, total, limit: 50, offset })

export const importe = (file: string, o: Partial<Releve> = {}): ResultatImport => ({ file, status: 'imported', statement: releve(o) })
export const dejaImporte = (file: string, o: Partial<Releve> = {}): ResultatImport => ({ file, status: 'already_imported', statement: releve(o) })
export const refuse = (file: string): ResultatImport => ({
  file, status: 'rejected',
  error: {
    code: 'checks_failed', message: 'relevé refusé : les contrôles ne passent pas', missing: [],
    gaps: [{ check: 'total_debits', expected: 239_125, read: 231_845 }, { check: 'new_balance', expected: 615_400, read: 622_680 }],
    unrecognized_lines: [{ page: 2, text: '12/08 CB*0000 RETR.DAB 60,0O' }, { page: 2, text: '19/08 VIR INST EXEMPLE 9,90' }, { page: 3, text: 'REF 44-A / 2,90' }],
  },
})

export const pdf = (nom: string, taille = 1200): File => {
  const f = new File(['%PDF-1.4'], nom, { type: 'application/pdf' })
  Object.defineProperty(f, 'size', { value: taille })
  return f
}

export const categorie = (o: Partial<Categorie> = {}): Categorie => ({ id: 1, key: null, name: 'Courses', kind: 'variable', monthly_cap: null, color: 1, position: 0, archived: false, operations: 0, merchants: 0, rules: 0, ...o })

export const CATEGORIES: Categorie[] = [
  categorie({ id: 1, key: 'logement', name: 'Logement', kind: 'fixe', monthly_cap: 78_000, color: 0 }),
  categorie({ id: 2, key: 'courses', name: 'Courses', monthly_cap: 38_000, color: 1 }),
  categorie({ id: 3, key: 'restaurants', name: 'Restaurants', monthly_cap: 15_000, color: 2 }),
  categorie({ id: 4, key: 'epargne', name: 'Épargne', kind: 'epargne', color: 7 }),
  categorie({ id: 5, key: 'revenus', name: 'Revenus', kind: 'revenu', color: 9 }),
  categorie({ id: 6, key: 'divers', name: 'Divers', color: 10, archived: true }),
]

export const regle = (o: Partial<Regle> = {}): Regle => ({ id: 1, key: null, keywords: ['MARCHE COLLINE'], type: null, section: null, category_id: 2, position: 0, operations: 2, ...o })
export const commercant = (o: Partial<Commercant> = {}): Commercant => ({ id: 1, name: 'Marche Colline Villeneuve', category_id: null, operations: 3, debits: 9360, credits: 0, last_date: '2026-11-26', variants: 1, ...o })
export const groupe = (o: Partial<Groupe> = {}): Groupe => ({ merchant_id: 7, merchant_name: 'Cafe Tamis Villeneuve', operations: 2, debits: 780, credits: 0, first_date: '2026-11-20', last_date: '2026-11-26', sample: [operation({ id: 2, label: 'CARTE 26/11 CAFE TAMIS VILLENEUVE', merchant: 'CAFE TAMIS VILLENEUVE', merchant_id: 7, merchant_name: 'Cafe Tamis Villeneuve', amount: 390 })], ...o })
export const file = (groups: Groupe[] = [groupe()]): FileAClasser => ({ operations: groups.reduce((n, g) => n + g.operations, 0), merchants: groups.length, debits: groups.reduce((n, g) => n + g.debits, 0), credits: groups.reduce((n, g) => n + g.credits, 0), groups })

export const periodeStats = (o: Partial<Periode> = {}): Periode => ({ id: '2026-11', from: '2026-11-01', to: '2026-11-30', days: 30, known_through: '2026-11-26', elapsed_days: 26, complete: false, open: true, statement_ids: [11], ...o })

export const ligneStats = (o: Partial<LigneCategorie> = {}): LigneCategorie => ({
  id: 3, name: 'Restaurants', kind: 'variable', color: 2, archived: false, monthly_cap: 15_000, amount: 23_680, operations: 20, share_bp: 1070,
  previous: { amount: 14_800, delta: 8880, delta_bp: 6000 }, avg3: { amount: 17_300, periods: 3, delta: 6380, delta_bp: 3688 }, avg12: { amount: 13_900, periods: 12, delta: 9780, delta_bp: 7036 },
  cap_delta: 8680, cap_bp: 15_787, trend: [11_200, 9800, 12_000, 13_100, 14_000, 12_600, 12_800, 15_500, 13_900, 13_300, 14_800, 23_680], ...o,
})

const cumul = (total: number, jours: number, connus = jours) => Array.from({ length: connus }, (_, i) => Math.round((total * (i < 4 ? (i + 1) * 0.02 : 0.4 + ((i - 4) / Math.max(1, connus - 5)) * 0.6)) / 100) * 100)

export const moisStats = (o: Partial<Mois> = {}): Mois => ({
  mode: 'calendar', period: periodeStats(), previous_id: '2026-10', next_id: null, periods: 12, operations: 84,
  expenses: { total: 221_340, fixed: 99_525, variable: 119_015, unclassified: 2800, fixed_bp: 4496, variable_bp: 5377, unclassified_bp: 127, previous: { amount: 202_700, delta: 18_640, delta_bp: 920 }, previous_to_date: { amount: 178_500, delta: 42_840, delta_bp: 2400 }, avg3: { amount: 204_400, periods: 3, delta: 16_940, delta_bp: 829 }, avg12: { amount: 194_200, periods: 11, delta: 27_140, delta_bp: 1398 } },
  income: { total: 284_000, classified: 284_000, unclassified: 0, unclassified_operations: 0, previous: { amount: 270_500, delta: 13_500, delta_bp: 499 } },
  savings: { total: 25_000, rate_bp: 880, previous: { amount: 30_000, delta: -5000, delta_bp: -1667 } },
  remaining: { total: 37_660, previous: { amount: 37_800, delta: -140, delta_bp: -37 } },
  budget: { total: 205_000, spent: 221_340, bp: 10_797, delta: 16_340, uncapped: 2800 },
  categories: [
    ligneStats({ id: 1, name: 'Logement', kind: 'fixe', color: 0, monthly_cap: 78_000, amount: 78_000, operations: 1, share_bp: 3524, previous: { amount: 78_000, delta: 0, delta_bp: 0 }, avg3: { amount: 78_000, periods: 3, delta: 0, delta_bp: 0 }, avg12: { amount: 77_300, periods: 11, delta: 700, delta_bp: 91 }, cap_delta: 0, cap_bp: 10_000, trend: [76_000, 76_000, 76_000, 78_000, 78_000, 78_000] }),
    ligneStats({ id: 2, name: 'Courses', color: 1, monthly_cap: 38_000, amount: 41_260, operations: 18, share_bp: 1864, previous: { amount: 37_100, delta: 4160, delta_bp: 1121 }, avg3: { amount: 38_200, periods: 3, delta: 3060, delta_bp: 801 }, avg12: { amount: 35_700, periods: 11, delta: 5560, delta_bp: 1557 }, cap_delta: 3260, cap_bp: 10_858, trend: [33_100, 32_200, 34_500, 35_000, 36_100, 37_100, 41_260] }),
    ligneStats(),
    ligneStats({ id: 6, name: 'Transports', color: 5, monthly_cap: 16_000, amount: 14_230, operations: 9, share_bp: 643, previous: { amount: 15_100, delta: -870, delta_bp: -576 }, avg3: { amount: 14_800, periods: 3, delta: -570, delta_bp: -385 }, avg12: { amount: 15_300, periods: 11, delta: -1070, delta_bp: -699 }, cap_delta: -1770, cap_bp: 8894, trend: [15_800, 14_900, 15_100, 14_230] }),
    ligneStats({ id: 7, name: 'Santé', color: 6, monthly_cap: null, amount: 4620, operations: 2, share_bp: 209, previous: { amount: 3800, delta: 820, delta_bp: 2158 }, avg3: null, avg12: null, cap_delta: null, cap_bp: null, trend: [3800, 4620] }),
  ],
  unclassified: { amount: 2800, operations: 7, share_bp: 127, previous: { amount: 0, delta: 2800, delta_bp: null } },
  overruns: [
    { category_id: 3, reference: 'cap', reference_amount: 15_000, periods: null, excess: 8680, excess_bp: 5787, rank_amount: 1, rank_percent: 1 },
    { category_id: 2, reference: 'cap', reference_amount: 38_000, periods: null, excess: 3260, excess_bp: 858, rank_amount: 2, rank_percent: 3 },
    { category_id: 7, reference: 'average', reference_amount: 3300, periods: 2, excess: 1320, excess_bp: 4000, rank_amount: 3, rank_percent: 2 },
  ],
  overrun_total: 13_260,
  cumulative: { current: cumul(221_340, 30, 26), previous: { id: '2026-10', values: cumul(202_700, 30) }, average: { periods: 11, values: cumul(194_200, 30) } },
  projection: { total: 243_000, variable: 137_300, fixed: 105_700, fixed_pending: 6175, daily_variable: 5400, remaining_days: 4, budget_delta: 38_000 },
  projection_reason: null,
  history: ['2025-12', '2026-01', '2026-02', '2026-03', '2026-04', '2026-05', '2026-06', '2026-07', '2026-08', '2026-09', '2026-10', '2026-11'].map((id, i) => ({ id, from: `${id}-01`, to: `${id}-28`, complete: i < 11, expenses: 180_000 + i * 3000, income: 270_000 + (i % 3) * 5000, savings: 20_000 + (i % 4) * 2500, remaining: 40_000 - i * 500, savings_rate_bp: 800 + i * 10 })),
  ...o,
})

export const detailStats = (o: Partial<DetailCategorie> = {}): DetailCategorie => ({
  mode: 'calendar', period: periodeStats(), previous_id: '2026-10', next_id: null, periods: 12, category: ligneStats(),
  history: ligneStats().trend.map((amount, i) => { const id = ['2025-12', '2026-01', '2026-02', '2026-03', '2026-04', '2026-05', '2026-06', '2026-07', '2026-08', '2026-09', '2026-10', '2026-11'][i]; return { id, from: `${id}-01`, to: `${id}-28`, complete: i < 11, amount } }),
  over_cap: 2,
  merchants: [
    { merchant_id: 21, name: 'Livreo', amount: 11_840, operations: 6, average: 1973, previous: { amount: 6170, delta: 5670, delta_bp: 9190 } },
    { merchant_id: 22, name: 'Bistrot Lune', amount: 7450, operations: 3, average: 2483, previous: { amount: 5400, delta: 2050, delta_bp: 3796 } },
    { merchant_id: null, name: null, amount: 4390, operations: 11, average: 399, previous: { amount: 0, delta: 4390, delta_bp: null } },
  ],
  merchants_total: 3,
  operations: [operation({ id: 31, date: '2026-11-24', purchase_date: '2026-11-22', merchant_name: 'Livreo', amount: 4180, category_id: 3 }), operation({ id: 32, date: '2026-11-22', purchase_date: null, merchant_name: 'Bistrot Lune', amount: 2850, category_id: 3 })],
  operations_total: 20,
  ...o,
})

const MOIS_ESSAI = ['2026-06', '2026-07', '2026-08', '2026-09', '2026-10', '2026-11']

export const tendanceStats = (o: Partial<Tendance> = {}): Tendance => ({ direction: 'up', periods: 5, average: 13_900, delta: 3000, delta_bp: 2158, values: [12_800, 15_500, 13_900, 13_300, 14_800], ...o })

export const ligneHistorique = (o: Partial<LigneHistorique> = {}): LigneHistorique => ({ id: 3, name: 'Restaurants', kind: 'variable', color: 2, archived: false, monthly_cap: 15_000, amounts: [12_800, 15_500, 13_900, 13_300, 14_800, 23_680], total: 93_980, average: 14_060, over_cap: 2, trend: tendanceStats(), ...o })

export const historiqueStats = (o: Partial<Historique> = {}): Historique => ({
  mode: 'calendar', window: '6', available: 12, complete: 5,
  periods: MOIS_ESSAI.map((id, i) => ({ id, from: `${id}-01`, to: `${id}-28`, complete: i < 5, open: i === 5, expenses: [144_800, 145_800, 146_500, 147_600, 148_800, 164_590][i] })),
  trend_period_ids: MOIS_ESSAI.slice(0, 5),
  categories: [
    ligneHistorique({ id: 1, name: 'Logement', kind: 'fixe', color: 0, monthly_cap: 78_000, amounts: [78_000, 78_000, 78_000, 78_000, 78_000, 78_000], total: 468_000, average: 78_000, over_cap: 0, trend: tendanceStats({ direction: 'stable', average: 78_000, delta: 0, delta_bp: 0, values: [78_000, 78_000, 78_000, 78_000, 78_000] }) }),
    ligneHistorique({ id: 2, name: 'Courses', color: 1, monthly_cap: 38_000, amounts: [35_200, 34_000, 36_100, 36_200, 37_100, 41_260], total: 219_860, average: 35_720, over_cap: 1, trend: tendanceStats({ direction: 'stable', average: 35_720, delta: 1900, delta_bp: 532, values: [35_200, 34_000, 36_100, 36_200, 37_100] }) }),
    ligneHistorique(),
    ligneHistorique({ id: 6, name: 'Transports', color: 5, monthly_cap: null, amounts: [15_800, 12_100, 16_000, 15_300, 15_100, 14_230], total: 88_530, average: 14_860, over_cap: null, trend: tendanceStats({ direction: 'down', average: 14_860, delta: -1600, delta_bp: -1077, values: [15_800, 12_100, 16_000, 15_300, 15_100] }) }),
    ligneHistorique({ id: 7, name: 'Santé', color: 6, monthly_cap: null, amounts: [3000, 6200, 2500, 4800, 3800, 4620], total: 24_920, average: 4060, over_cap: null, trend: tendanceStats({ direction: 'stable', average: 4060, delta: 800, delta_bp: 1970, values: [3000, 6200, 2500, 4800, 3800] }) }),
  ],
  unclassified: { amounts: [0, 0, 0, 0, 0, 2800], total: 2800, average: 0 },
  total: { amounts: [144_800, 145_800, 146_500, 147_600, 148_800, 164_590], total: 898_090, average: 146_700, budget: 131_000, over_budget: 6, trend: tendanceStats({ direction: 'stable', average: 146_700, delta: 4000, delta_bp: 273, values: [144_800, 145_800, 146_500, 147_600, 148_800] }) },
  chart: { category_ids: [1, 2, 3, 6], others: [3000, 6200, 2500, 4800, 3800, 4620], others_categories: 1 },
  ...o,
})

export const soldesStats = (o: Partial<Soldes> = {}): Soldes => ({
  mode: 'calendar', window: '6', from: '2026-10-01', to: '2026-11-26',
  accounts: [{
    account_id: 1, days: 52, reconciled: true,
    segments: [{ from: '2026-10-01', to: '2026-10-26', values: Array.from({ length: 26 }, (_, i) => (i < 4 ? 120_000 : i < 20 ? -4500 : 152_340)) }, { from: '2026-11-01', to: '2026-11-26', values: Array.from({ length: 26 }, (_, i) => (i < 10 ? 152_340 : i < 22 ? 61_000 : 190_000)) }],
    gaps: [{ from: '2026-10-27', to: '2026-10-31', days: 5 }],
    first: { date: '2026-10-01', amount: 120_000 }, last: { date: '2026-11-26', amount: 190_000 }, min: { date: '2026-10-05', amount: -4500 }, max: { date: '2026-11-23', amount: 190_000 },
  }],
  ...o,
})

export const anneesStats = (o: Partial<Annees> = {}): Annees => ({
  mode: 'calendar', period: '2026-11', year: 2026, month: 11, previous_year: 2025, has_previous: true, compared_months: [10, 11], partial: true,
  months: Array.from({ length: 12 }, (_, i) => ({ month: i + 1, current: i >= 5 && i <= 10 ? [188_400, 193_700, 196_500, 189_400, 202_700, 221_340][i - 5] : null, current_complete: i >= 5 && i <= 10 ? i < 10 : null, previous: i >= 9 ? [199_000, 204_000, 216_500][i - 9] : null, previous_complete: i >= 9 ? true : null })),
  total: { month: 221_340, month_previous: { amount: 204_000, delta: 17_340, delta_bp: 850 }, ytd: 1_192_040, ytd_compared: { current: 424_040, previous: 403_000, delta: 21_040, delta_bp: 522 } },
  categories: [
    { id: 1, name: 'Logement', kind: 'fixe', color: 0, month: 78_000, month_previous: { amount: 76_000, delta: 2000, delta_bp: 263 }, ytd: 468_000, ytd_compared: { current: 156_000, previous: 152_000, delta: 4000, delta_bp: 263 } },
    { id: 3, name: 'Restaurants', kind: 'variable', color: 2, month: 23_680, month_previous: { amount: 0, delta: 23_680, delta_bp: null }, ytd: 93_980, ytd_compared: { current: 38_480, previous: 0, delta: 38_480, delta_bp: null } },
  ],
  unclassified: { month: 2800, month_previous: { amount: 0, delta: 2800, delta_bp: null }, ytd: 2800, ytd_compared: { current: 2800, previous: 0, delta: 2800, delta_bp: null } },
  ...o,
})

export const fauxClient = (o: Partial<ClientTirelire> = {}): ClientTirelire => ({
  me: async () => ({ sub: 'kc-moi', name: 'Camille', email: '', permissions: { access: true } }),
  comptes: async () => [],
  releves: async () => [],
  importer: async () => [],
  supprimer: async () => {},
  operations: async () => page([]),
  exporter: async () => ({ blob: new Blob(['']), nom: 'tirelire-operations.csv' }),
  operation: async () => operation(),
  classerOperation: async () => ({ operation: operation(), reclassified: 1 }),
  aClasser: async () => file([]),
  compteur: async () => ({ operations: 0, merchants: 0, debits: 0, credits: 0 }),
  categories: async () => CATEGORIES,
  creerCategorie: async () => categorie(),
  modifierCategorie: async () => categorie(),
  regles: async () => [],
  creerRegle: async () => ({ rule: regle(), reclassified: 0 }),
  modifierRegle: async () => ({ rule: regle(), reclassified: 0 }),
  supprimerRegle: async () => ({ reclassified: 0 }),
  ordonnerRegles: async () => ({ rules: [], reclassified: 0 }),
  commercants: async () => [],
  commercant: async () => commercant(),
  classerCommercant: async () => ({ merchant: commercant(), reclassified: 0 }),
  fusionner: async () => ({ merchant: commercant(), reclassified: 0 }),
  separer: async () => ({ merchant: commercant(), created: [] }),
  periodes: async () => [],
  mois: async () => moisStats(),
  categorieDuMois: async () => detailStats(),
  historique: async () => historiqueStats(),
  soldes: async () => soldesStats(),
  annees: async () => anneesStats(),
  ...o,
})
