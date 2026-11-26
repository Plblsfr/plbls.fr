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
  merchant_name: string | null
  category_id: number | null
  category_source: SourceCategorie | null
  rule_id: number | null
}

export type SourceCategorie = 'operation' | 'commercant' | 'regle'
export type Nature = 'fixe' | 'variable' | 'epargne' | 'revenu' | 'neutre'
export type Portee = 'operation' | 'merchant'

export interface Categorie {
  id: number
  key: string | null
  name: string
  kind: Nature
  monthly_cap: number | null
  color: number
  position: number
  archived: boolean
  operations: number
  merchants: number
  rules: number
}

export interface Regle { id: number; key: string | null; keywords: string[]; type: TypeOperation | null; section: string | null; category_id: number; position: number; operations: number }
export interface Variante { key: string; manual: boolean; operations: number }
export interface Commercant { id: number; name: string; category_id: number | null; operations: number; debits: number; credits: number; last_date: string | null; variants: number; variant_list?: Variante[] }
export interface Compteur { operations: number; merchants: number; debits: number; credits: number }
export interface Groupe { merchant_id: number | null; merchant_name: string | null; operations: number; debits: number; credits: number; first_date: string; last_date: string; sample: Operation[] }
export interface FileAClasser extends Compteur { groups: Groupe[] }

export interface SaisieCategorie { name: string; kind: Nature; monthly_cap: number | null }
export interface SaisieRegle { keywords: string[]; category_id: number }

export interface PageOperations { operations: Operation[]; total: number; limit: number; offset: number }
export interface Filtres { releve: number | null; du: string; au: string; categorie: number | null; commercant: number | null; aClasser: boolean }
export const SANS_FILTRE: Filtres = { releve: null, du: '', au: '', categorie: null, commercant: null, aClasser: false }
export const filtres = (f: Partial<Filtres> = {}): Filtres => ({ ...SANS_FILTRE, ...f })

export type CodeRefus = 'not_a_pdf' | 'unreadable_pdf' | 'too_large' | 'unknown_format' | 'conflict' | 'checks_failed'
export interface Refus { code: CodeRefus | (string & {}); message: string; missing?: string[]; gaps?: Ecart[]; unrecognized_lines?: LigneInconnue[] }

export type ResultatImport =
  | { file: string; status: 'imported'; statement: Releve }
  | { file: string; status: 'already_imported'; statement: Releve }
  | { file: string; status: 'rejected'; error: Refus }

export type Issue = ResultatImport | { file: string; status: 'failed'; message: string }

const get = <T>(chemin: string) => requestAt<T>(TIRELIRE_URL, 'GET', chemin)
const envoyer = <T>(methode: string, chemin: string, corps?: unknown) => requestAt<T>(TIRELIRE_URL, methode, chemin, corps)

export function requeteOperations(f: Filtres, offset = 0, limit = PAGE): string {
  const q = new URLSearchParams()
  if (f.releve !== null) q.set('statement', String(f.releve))
  if (f.du) q.set('from', f.du)
  if (f.au) q.set('to', f.au)
  if (f.categorie !== null) q.set('category', String(f.categorie))
  if (f.commercant !== null) q.set('merchant', String(f.commercant))
  if (f.aClasser) q.set('unclassified', 'true')
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
  operation: async (id: number) => (await get<{ operation: Operation }>(`/operations/${id}`)).operation,
  classerOperation: (id: number, categorie: number | null, portee: Portee) => envoyer<{ operation: Operation; reclassified: number }>('PUT', `/operations/${id}/category`, { category_id: categorie, scope: portee }),
  aClasser: () => get<FileAClasser>('/unclassified'),
  compteur: () => get<Compteur>('/unclassified/count'),
  categories: async () => (await get<{ categories: Categorie[] }>('/categories')).categories,
  creerCategorie: async (c: SaisieCategorie) => (await envoyer<{ category: Categorie }>('POST', '/categories', c)).category,
  modifierCategorie: async (id: number, c: Partial<SaisieCategorie & { archived: boolean }>) => (await envoyer<{ category: Categorie }>('PATCH', `/categories/${id}`, c)).category,
  regles: async () => (await get<{ rules: Regle[] }>('/rules')).rules,
  creerRegle: (r: SaisieRegle) => envoyer<{ rule: Regle; reclassified: number }>('POST', '/rules', r),
  modifierRegle: (id: number, r: Partial<SaisieRegle>) => envoyer<{ rule: Regle; reclassified: number }>('PATCH', `/rules/${id}`, r),
  supprimerRegle: (id: number) => envoyer<{ reclassified: number }>('DELETE', `/rules/${id}`),
  ordonnerRegles: (ids: number[]) => envoyer<{ rules: Regle[]; reclassified: number }>('PUT', '/rules/order', { ids }),
  commercants: async (q = '') => (await get<{ merchants: Commercant[] }>(`/merchants${q ? `?q=${encodeURIComponent(q)}` : ''}`)).merchants,
  commercant: async (id: number) => (await get<{ merchant: Commercant }>(`/merchants/${id}`)).merchant,
  classerCommercant: (id: number, categorie: number | null) => envoyer<{ merchant: Commercant; reclassified: number }>('PUT', `/merchants/${id}/category`, { category_id: categorie }),
  fusionner: (id: number, autres: number[]) => envoyer<{ merchant: Commercant; reclassified: number }>('POST', `/merchants/${id}/merge`, { merchant_ids: autres }),
  separer: (id: number, cles: string[]) => envoyer<{ merchant: Commercant; created: Commercant[] }>('POST', `/merchants/${id}/split`, { keys: cles }),
}

export type ClientTirelire = typeof tirelireApi

export function messageErreur(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 401) return 'Ta session a expiré. Reconnecte-toi pour continuer.'
    if (e.code === 'bad_origin') return 'Cette adresse de l’app n’est pas autorisée par Tirelire.'
    if (e.status === 403) return 'Tu n’as pas accès à Tirelire.'
    if (e.status === 404) return 'Cet élément n’existe plus.'
    if (e.status === 413 || e.code === 'too_large') return 'Envoi trop volumineux : 8 Mo au plus par fichier, 32 Mo par envoi.'
    if (e.status === 409) return 'Une catégorie porte déjà ce nom.'
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

export const titreDe = (o: Pick<Operation, 'merchant' | 'label'> & { merchant_name?: string | null }) => o.merchant_name?.trim() || o.merchant?.trim() || o.label

export const NATURES: Record<Nature, string> = { fixe: 'fixe', variable: 'variable', epargne: 'épargne', revenu: 'revenu', neutre: 'neutre' }
export const NATURES_LONGUES: Record<Nature, string> = { fixe: 'Dépense fixe', variable: 'Dépense variable', epargne: 'Épargne', revenu: 'Revenu', neutre: 'Transfert neutre' }
export const ORDRE_NATURES: Nature[] = ['fixe', 'variable', 'epargne', 'revenu', 'neutre']
export const estDepense = (c: Pick<Categorie, 'kind'>) => c.kind === 'fixe' || c.kind === 'variable'

export const TEINTES = 8
export const teinte = (c: Pick<Categorie, 'color'> | null | undefined) => (c && c.color >= 0 && c.color < TEINTES ? c.color + 1 : 0)

export const actives = (categories: Categorie[]) => categories.filter((c) => !c.archived)

export const budget = (categories: Categorie[]) => actives(categories).filter(estDepense).reduce((n, c) => n + (c.monthly_cap ?? 0), 0)

export const SOURCES: Record<SourceCategorie, string> = { operation: 'choisie pour cette opération', commercant: 'règle du commerçant', regle: 'règle par mots-clés' }

export function conditionDe(r: Pick<Regle, 'type' | 'section'>): string {
  const parts: string[] = []
  if (r.type) parts.push(`type « ${(TYPES[r.type] ?? r.type).toLowerCase()} »`)
  if (r.section) parts.push(`section « ${r.section} »`)
  return parts.join(' · ')
}

export const motCle = (texte: string) => texte.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toUpperCase().replace(/[^A-Z0-9]+/g, ' ').trim()

export const resteDe = (g: Pick<Groupe, 'debits' | 'credits'>) => g.credits - g.debits

export function deplacer<T>(liste: T[], de: number, vers: number): T[] {
  if (de === vers || de < 0 || vers < 0 || de >= liste.length || vers >= liste.length) return liste
  const copie = [...liste]
  const [e] = copie.splice(de, 1)
  copie.splice(vers, 0, e)
  return copie
}

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
  const r = f.releve !== null && !f.du && !f.au && f.categorie === null && f.commercant === null && !f.aClasser ? releves.find((x) => x.id === f.releve) : undefined
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
