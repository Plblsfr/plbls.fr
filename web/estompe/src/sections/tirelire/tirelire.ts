import { ApiError, requestAt } from '../../lib/api'
import { config } from '../../lib/config'
import { signe, type Sens } from './formats'

export const TIRELIRE_URL = config.tirelireUrl
export const tirelireOn = () => !!TIRELIRE_URL

export const FICHIER_MAX = 8 * 1024 * 1024
export const ENVOI_MAX = 32 * 1024 * 1024
export const FICHIERS_MAX = 24
const MARGE_ENVOI = 1024 * 1024
export const PAGE = 50

export interface TirelireMe { sub: string; name: string; email: string; permissions: { access: boolean } }

export interface Compte { id: number; bank: string; iban: string; bic: string | null; number: string | null; reference: string | null; holder: string | null; statements: number }

export type Controle = 'total_debits' | 'total_credits' | 'new_balance'
export interface Lecture { expected: number; read: number }
export interface Ecart extends Lecture { check: Controle }
export interface LigneInconnue { page: number; text: string }
export interface Rapport { total_debits: Lecture; total_credits: Lecture; new_balance: Lecture; operations: number; unrecognized_lines: number }

export interface Releve {
  id: number
  account_id: number
  number: number
  year: number
  from: string
  to: string
  old_balance: number
  new_balance: number
  total_debits: number
  total_credits: number
  operations: number
  pages: number
  sha256: string
  checks: Rapport
  imported_at: string
}

export type TypeOperation = 'carte' | 'prelevement' | 'virement' | 'virement_instantane' | 'echeance_pret' | 'cotisation' | 'autre'

export interface Operation {
  id: number
  statement_id: number
  account_id: number
  date: string
  value_date: string
  purchase_date: string | null
  section: string
  type: TypeOperation
  label: string
  details: string[]
  merchant: string | null
  amount: number
  direction: Sens
  merchant_id: number | null
  category_id: number | null
}

export interface PageOperations { operations: Operation[]; total: number; limit: number; offset: number }
export interface Filtres { releve: number | null; du: string; au: string }

export type CodeRefus = 'not_a_pdf' | 'unreadable_pdf' | 'too_large' | 'unknown_format' | 'conflict' | 'checks_failed'
export interface Refus { code: CodeRefus | (string & {}); message: string; missing?: string[]; gaps?: Ecart[]; unrecognized_lines?: LigneInconnue[] }

export type ResultatImport =
  | { file: string; status: 'imported'; statement: Releve }
  | { file: string; status: 'already_imported'; statement: Releve }
  | { file: string; status: 'rejected'; error: Refus }

export type Issue = ResultatImport | { file: string; status: 'failed'; message: string }

const get = <T>(chemin: string) => requestAt<T>(TIRELIRE_URL, 'GET', chemin)

export function requeteOperations(f: Filtres, offset = 0, limit = PAGE): string {
  const q = new URLSearchParams()
  if (f.releve !== null) q.set('statement', String(f.releve))
  if (f.du) q.set('from', f.du)
  if (f.au) q.set('to', f.au)
  q.set('limit', String(limit))
  q.set('offset', String(offset))
  return `/operations?${q}`
}

export const tirelireApi = {
  me: () => requestAt<TirelireMe>(TIRELIRE_URL, 'GET', '/me', undefined, { discret: true }),
  comptes: async () => (await get<{ accounts: Compte[] }>('/accounts')).accounts,
  releves: async () => (await get<{ statements: Releve[] }>('/statements')).statements,
  importer: async (fichiers: File[]) => {
    const corps = new FormData()
    for (const f of fichiers) corps.append('file', f, f.name)
    return (await requestAt<{ results: ResultatImport[] }>(TIRELIRE_URL, 'POST', '/statements/import', corps)).results
  },
  supprimer: (id: number) => requestAt<void>(TIRELIRE_URL, 'DELETE', `/statements/${id}`),
  operations: (f: Filtres, offset = 0, limit = PAGE) => get<PageOperations>(requeteOperations(f, offset, limit)),
}

export type ClientTirelire = typeof tirelireApi

export function messageErreur(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 401) return 'Ta session a expiré. Reconnecte-toi pour continuer.'
    if (e.code === 'bad_origin') return 'Cette adresse de l’app n’est pas autorisée par Tirelire.'
    if (e.status === 403) return 'Tu n’as pas accès à Tirelire.'
    if (e.status === 404) return 'Ce relevé n’existe plus.'
    if (e.status === 413 || e.code === 'too_large') return 'Envoi trop volumineux : 8 Mo au plus par fichier, 32 Mo par envoi.'
    if (e.status === 400) return 'Tirelire n’a pas pu lire la demande.'
    if (e.code === 'timeout') return 'Tirelire ne répond pas. Réessaie dans un instant.'
    if (e.status >= 500) return 'Tirelire est indisponible pour le moment.'
    return e.message || 'Tirelire a refusé la demande.'
  }
  return 'Tirelire est injoignable. Vérifie ta connexion.'
}

const REFUS: Record<CodeRefus, string> = {
  not_a_pdf: 'Ce fichier n’est pas un PDF.',
  unreadable_pdf: 'Ce PDF est illisible.',
  too_large: 'Ce fichier dépasse 8 Mo.',
  unknown_format: 'Ce n’est pas un relevé de compte du Crédit Mutuel de Bretagne.',
  conflict: 'Ce relevé est déjà en cours d’import. Réessaie dans un instant.',
  checks_failed: 'Les totaux lus ne correspondent pas aux totaux imprimés sur le relevé.',
}

export const messageRefus = (r: Refus) => REFUS[r.code as CodeRefus] ?? r.message

export const CONTROLES: Record<Controle, string> = { total_debits: 'Débits', total_credits: 'Crédits', new_balance: 'Solde final' }

export const TYPES: Record<TypeOperation, string> = {
  carte: 'Carte',
  prelevement: 'Prélèvement',
  virement: 'Virement',
  virement_instantane: 'Virement instantané',
  echeance_pret: 'Échéance de prêt',
  cotisation: 'Cotisation',
  autre: 'Autre opération',
}

export const typeDe = (o: Pick<Operation, 'type'>) => TYPES[o.type] ?? TYPES.autre

export const natureDe = (o: Pick<Operation, 'type' | 'direction'>) => `${o.direction === 'debit' ? 'débit' : 'crédit'} · ${typeDe(o).toLowerCase()}`

export const titreDe = (o: Pick<Operation, 'merchant' | 'label'>) => o.merchant?.trim() || o.label

export const nomReleve = (r: Pick<Releve, 'number' | 'year'>) => `Relevé n° ${r.number} de ${r.year}`

export interface Lots { lots: File[][]; tropGros: File[] }

export function lots(fichiers: File[]): Lots {
  const tropGros: File[] = []
  const res: File[][] = []
  let courant: File[] = []
  let taille = 0
  for (const f of fichiers) {
    if (f.size > FICHIER_MAX) { tropGros.push(f); continue }
    if (courant.length && (courant.length >= FICHIERS_MAX || taille + f.size > ENVOI_MAX - MARGE_ENVOI)) { res.push(courant); courant = []; taille = 0 }
    courant.push(f)
    taille += f.size
  }
  if (courant.length) res.push(courant)
  return { lots: res, tropGros }
}

export interface Jour { jour: string; operations: Operation[]; solde: number }

export function parJour(operations: Operation[]): Jour[] {
  const jours: Jour[] = []
  for (const o of operations) {
    let j = jours.at(-1)
    if (!j || j.jour !== o.date) { j = { jour: o.date, operations: [], solde: 0 }; jours.push(j) }
    j.operations.push(o)
    j.solde += signe(o.amount, o.direction)
  }
  return jours
}

export interface Totaux { debits: number; credits: number }

export function totaux(operations: Operation[]): Totaux {
  let debits = 0
  let credits = 0
  for (const o of operations) {
    if (o.direction === 'debit') debits += o.amount
    else credits += o.amount
  }
  return { debits, credits }
}

export function totauxConnus(f: Filtres, releves: Releve[], operations: Operation[], total: number): Totaux | null {
  if (operations.length >= total) return totaux(operations)
  const r = f.releve !== null && !f.du && !f.au ? releves.find((x) => x.id === f.releve) : undefined
  return r ? { debits: r.total_debits, credits: r.total_credits } : null
}

export interface Bilan { importes: number; deja: number; refuses: number; operations: number }

export function bilan(issues: Issue[]): Bilan {
  const b: Bilan = { importes: 0, deja: 0, refuses: 0, operations: 0 }
  for (const i of issues) {
    if (i.status === 'imported') { b.importes += 1; b.operations += i.statement.operations }
    else if (i.status === 'already_imported') b.deja += 1
    else b.refuses += 1
  }
  return b
}

export function raisonControle(r: Refus): string {
  const raisons: string[] = []
  if (r.gaps?.length) raisons.push('les totaux lus ne correspondent pas aux totaux imprimés sur le relevé')
  if (r.unrecognized_lines?.length) raisons.push(r.unrecognized_lines.length > 1 ? 'des lignes du relevé n’ont pas été reconnues' : 'une ligne du relevé n’a pas été reconnue')
  if (r.missing?.length) raisons.push('des éléments du relevé sont introuvables')
  if (!raisons.length) return 'le relevé ne passe pas les vérifications.'
  return `${raisons.length > 1 ? `${raisons.slice(0, -1).join(', ')} et ${raisons.at(-1)}` : raisons[0]}.`
}

const RANG: Record<Issue['status'], number> = { rejected: 0, failed: 0, imported: 1, already_imported: 2 }

export const parGravite = (issues: Issue[]) => [...issues].sort((a, b) => RANG[a.status] - RANG[b.status])
