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
  ...o,
})
