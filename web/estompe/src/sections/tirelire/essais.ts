import type { ClientTirelire, Operation, PageOperations, Releve, ResultatImport } from './tirelire'

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
  amount: 3120, direction: 'debit', merchant_id: null, category_id: null,
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

export const fauxClient = (o: Partial<ClientTirelire> = {}): ClientTirelire => ({
  me: async () => ({ sub: 'kc-moi', name: 'Camille', email: '', permissions: { access: true } }),
  comptes: async () => [],
  releves: async () => [],
  importer: async () => [],
  supprimer: async () => {},
  operations: async () => page([]),
  ...o,
})
