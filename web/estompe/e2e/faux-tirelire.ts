import type { Page, Route } from '@playwright/test'
import { simuler, type Profil } from './faux-sablier'

export const TIRELIRE = 'http://tirelire.test'
export const PORT = 4179

type Sens = 'debit' | 'credit'
interface Ligne { jour: string; type: string; section: string; label: string; details: string[]; merchant: string | null; achat: string | null; amount: number; direction: Sens }

const COMMERCES = ['MARCHE COLLINE VILLENEUVE', 'CAFE TAMIS VILLENEUVE', 'BISTROT LUNE SAINT-ELOI', 'CINEMA LE PHARE VILLENEUVE', 'LIBRAIRIE DES QUAIS SAINT-ELOI', 'BOULANGERIE DU PONT VILLENEUVE', 'MARCHE COLLINE SAINT-ELOI']
const jour = (annee: number, mois: number, j: number) => `${annee}-${String(mois).padStart(2, '0')}-${String(j).padStart(2, '0')}`

function lignes(annee: number, mois: number, nombre: number): Ligne[] {
  const res: Ligne[] = []
  for (let i = 0; i < nombre; i++) {
    const j = 26 - Math.floor(i / 3)
    if (i % 17 === 5) res.push({ jour: jour(annee, mois, j), type: 'virement', section: 'VIREMENTS RECUS', label: 'VIR ATELIER MERIDIEN SALAIRE', details: ['SALAIRE DU MOIS'], merchant: null, achat: null, amount: 264_000, direction: 'credit' })
    else if (i % 11 === 7) res.push({ jour: jour(annee, mois, j), type: 'prelevement', section: 'VIREMENTS EMIS ET PRELEVEMENTS', label: 'PRLV STREAMO ABONNEMENT', details: ['REF EXEMPLE-0001', 'MANDAT EXEMPLE-0002'], merchant: null, achat: null, amount: 1399, direction: 'debit' })
    else {
      const commerce = COMMERCES[i % COMMERCES.length]
      const achat = Math.max(1, j - (i % 3))
      res.push({ jour: jour(annee, mois, j), type: 'carte', section: 'PAIEMENTS PAR CARTE', label: `CARTE ${String(achat).padStart(2, '0')}/${String(mois).padStart(2, '0')} ${commerce}`, details: i % 4 === 0 ? ['REF EXEMPLE ' + String(1000 + i)] : [], merchant: commerce, achat: jour(annee, mois, achat), amount: 390 + ((i * 731) % 5200), direction: 'debit' })
    }
  }
  return res
}

type Nature = 'fixe' | 'variable' | 'epargne' | 'revenu' | 'neutre'
interface Categorie { id: number; key: string | null; name: string; kind: Nature; monthly_cap: number | null; color: number; position: number; archived: boolean }
interface Regle { id: number; key: string | null; keywords: string[]; type: string | null; section: string | null; category_id: number }
interface Commercant { id: number; name: string; category_id: number | null; keys: string[] }
type Op = Record<string, unknown> & { id: number; label: string; type: string; merchant: string | null; amount: number; direction: Sens; date: string; cle: string; exception: boolean; merchant_id: number | null; category_id: number | null; category_source: string | null; rule_id: number | null }

const normaliser = (t: string) => t.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toUpperCase().replace(/[^A-Z0-9]+/g, ' ').trim()
const cleDe = (o: { type: string; label: string; merchant: string | null }) => normaliser(o.merchant ?? o.label.replace(/^(PRLV|VIR)( SEPA| INST)?( VERS| DE)? /, ''))
const nomDe = (cle: string) => cle.split(' ').map((m) => m.charAt(0) + m.slice(1).toLowerCase()).join(' ')

const categoriesParDefaut = (): Categorie[] => ([
  ['logement', 'Logement', 'fixe', 78_000], ['courses', 'Courses', 'variable', 38_000], ['restaurants', 'Restaurants', 'variable', 15_000], ['shopping', 'Shopping', 'variable', null],
  ['loisirs', 'Loisirs et sorties', 'variable', 12_000], ['transports', 'Transports', 'variable', null], ['sante', 'Santé', 'variable', null], ['epargne', 'Épargne', 'epargne', null],
  ['abonnements', 'Abonnements', 'fixe', 11_000], ['frais', 'Frais bancaires', 'fixe', null], ['revenus', 'Revenus', 'revenu', null], ['transferts', 'Transferts neutres', 'neutre', null],
] as [string, string, Nature, number | null][]).map(([key, name, kind, monthly_cap], i) => ({ id: i + 1, key, name, kind, monthly_cap, color: i, position: i, archived: false }))

const reglesParDefaut = (): Regle[] => [
  { id: 1, key: 'type-echeance-pret', keywords: [], type: 'echeance_pret', section: null, category_id: 1 },
  { id: 2, key: 'courses', keywords: ['BOULANGERIE', 'EPICERIE'], type: null, section: null, category_id: 2 },
  { id: 3, key: 'loisirs', keywords: ['CINEMA', 'LIBRAIRIE'], type: null, section: null, category_id: 5 },
  { id: 4, key: 'revenus', keywords: ['SALAIRE'], type: null, section: null, category_id: 11 },
]

export function serveurTirelire(avecReleves = false) {
  const srv = {
    categories: categoriesParDefaut(),
    regles: reglesParDefaut(),
    commercants: [] as Commercant[],
    suivant: 100,
    rejouer() {
      let changees = 0
      for (const o of srv.operations as Op[]) {
        let m = srv.commercants.find((c) => c.keys.includes(o.cle))
        if (!m) { m = { id: srv.suivant++, name: nomDe(o.cle), category_id: null, keys: [o.cle] }; srv.commercants.push(m) }
        o.merchant_id = m.id
        o.merchant_name = m.name
        if (o.exception) continue
        const texte = ` ${normaliser(o.label)} `
        const regle = m.category_id === null ? srv.regles.find((r) => (!r.type || r.type === o.type) && (!r.keywords.length || r.keywords.some((k) => texte.includes(` ${k} `)))) : undefined
        const voulu = m.category_id !== null ? [m.category_id, 'commercant', null] : regle ? [regle.category_id, 'regle', regle.id] : [null, null, null]
        if (voulu[0] !== o.category_id || voulu[1] !== o.category_source || voulu[2] !== o.rule_id) changees++
        ;[o.category_id, o.category_source, o.rule_id] = voulu as [number | null, string | null, number | null]
      }
      return changees
    },
    acces: true,
    arrete: false,
    releves: [] as Record<string, unknown>[],
    operations: [] as Record<string, unknown>[],
    fichiers: new Map<string, number>(),
    appels: [] as { methode: string; chemin: string; requete: string }[],
    prochain: 1,
    ajouter(fichier: string, numero: number, annee: number, mois: number, nombre: number) {
      const id = 100 + numero
      const ops = lignes(annee, mois, nombre)
      const debits = ops.filter((o) => o.direction === 'debit').reduce((n, o) => n + o.amount, 0)
      const credits = ops.filter((o) => o.direction === 'credit').reduce((n, o) => n + o.amount, 0)
      const releve = {
        id, account_id: 1, number: numero, year: annee, from: jour(annee, mois, 1), to: jour(annee, mois, 26), old_balance: 152_340, new_balance: 152_340 + credits - debits,
        total_debits: debits, total_credits: credits, operations: ops.length, pages: 3, sha256: String(id).padStart(64, '0'),
        checks: { total_debits: { expected: debits, read: debits }, total_credits: { expected: credits, read: credits }, new_balance: { expected: 152_340 + credits - debits, read: 152_340 + credits - debits }, operations: ops.length, unrecognized_lines: 0 },
        imported_at: `${jour(annee, mois, 26)}T09:30:00Z`,
      }
      srv.releves.push(releve)
      srv.releves.sort((a, b) => String(b.to).localeCompare(String(a.to)))
      for (const o of ops) srv.operations.push({ id: srv.prochain++, statement_id: id, account_id: 1, date: o.jour, value_date: o.jour, purchase_date: o.achat, section: o.section, type: o.type, label: o.label, details: o.details, merchant: o.merchant, amount: o.amount, direction: o.direction, merchant_id: null, merchant_name: null, category_id: null, category_source: null, rule_id: null, cle: cleDe({ type: o.type, label: o.label, merchant: o.merchant }), exception: false })
      srv.operations.sort((a, b) => String(b.date).localeCompare(String(a.date)) || Number(a.id) - Number(b.id))
      srv.fichiers.set(fichier, id)
      srv.rejouer()
      return releve
    },
  }
  if (avecReleves) {
    srv.ajouter('releve-2026-10.pdf', 10, 2026, 10, 40)
    srv.ajouter('releve-2026-11.pdf', 11, 2026, 11, 64)
  }
  return srv
}
export type Tirelire = ReturnType<typeof serveurTirelire>

const MOIS = /releve-(\d{4})-(\d{2})/

function importer(srv: Tirelire, fichier: string) {
  if (fichier.includes('refus')) {
    return {
      file: fichier, status: 'rejected',
      error: {
        code: 'checks_failed', message: 'relevé refusé : les contrôles ne passent pas', missing: [],
        gaps: [{ check: 'total_debits', expected: 239_125, read: 231_845 }, { check: 'new_balance', expected: 615_400, read: 622_680 }],
        unrecognized_lines: [{ page: 2, text: '12/08 CB*0000 RETR.DAB 60,0O' }, { page: 2, text: '19/08 VIR INST EXEMPLE 9,90' }, { page: 3, text: 'REF 44-A / 2,90' }],
      },
    }
  }
  if (fichier.includes('autre-banque')) return { file: fichier, status: 'rejected', error: { code: 'unknown_format', message: 'ce n’est pas un relevé du Crédit Mutuel de Bretagne' } }
  const connu = srv.fichiers.get(fichier)
  if (connu !== undefined) return { file: fichier, status: 'already_imported', statement: srv.releves.find((r) => r.id === connu) }
  const m = MOIS.exec(fichier)
  const annee = m ? Number(m[1]) : 2026
  const mois = m ? Number(m[2]) : 9
  return { file: fichier, status: 'imported', statement: srv.ajouter(fichier, mois, annee, mois, 30 + mois) }
}

export async function simulerTirelire(page: Page, p: Profil & { tirelire?: boolean } = {}, srv: Tirelire = serveurTirelire()) {
  await simuler(page, { acces: false, ...p }, undefined, PORT)
  const origine = new URL(page.url() === 'about:blank' ? `http://localhost:${PORT}` : page.url()).origin
  const cors = { 'access-control-allow-origin': origine, 'access-control-allow-credentials': 'true' }
  const json = (route: Route, body: unknown, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body), headers: cors })
  srv.acces = p.tirelire !== false
  await page.route(`${TIRELIRE}/**`, (route) => {
    const req = route.request()
    if (req.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: { ...cors, 'access-control-allow-headers': '*', 'access-control-allow-methods': '*' } })
    if (srv.arrete) return route.abort('connectionrefused')
    const url = new URL(req.url())
    srv.appels.push({ methode: req.method(), chemin: url.pathname, requete: url.search })
    if (url.pathname === '/me') return json(route, { sub: 'kc-moi', name: 'Camille', email: '', permissions: { access: srv.acces } })
    if (!srv.acces) return json(route, { error: 'forbidden', message: 'permission tirelire:access manquante' }, 403)
    if (url.pathname === '/accounts') return json(route, { accounts: srv.releves.length ? [{ id: 1, bank: 'cmb', iban: 'FR00 0000 0000 0000 0000 0000 000', bic: null, number: null, reference: null, holder: 'Camille Exemple', statements: srv.releves.length }] : [] })
    if (url.pathname === '/statements' && req.method() === 'GET') return json(route, { statements: srv.releves })
    if (url.pathname === '/statements/import') {
      const corps = req.postDataBuffer()?.toString('latin1') ?? ''
      const fichiers = [...corps.matchAll(/name="file"; filename="([^"]+)"/g)].map((m) => Buffer.from(m[1], 'latin1').toString('utf8'))
      return json(route, { results: fichiers.map((f) => importer(srv, f)) })
    }
    const suppression = /^\/statements\/(\d+)$/.exec(url.pathname)
    if (suppression && req.method() === 'DELETE') {
      const id = Number(suppression[1])
      if (!srv.releves.some((r) => r.id === id)) return json(route, { error: 'not_found', message: 'relevé inconnu' }, 404)
      srv.releves = srv.releves.filter((r) => r.id !== id)
      srv.operations = srv.operations.filter((o) => o.statement_id !== id)
      for (const [f, r] of srv.fichiers) if (r === id) srv.fichiers.delete(f)
      return route.fulfill({ status: 204, headers: cors })
    }
    const ops = srv.operations as Op[]
    const corps = () => (req.postDataJSON() ?? {}) as Record<string, unknown>
    const methode = req.method()
    const inconnu = () => json(route, { error: 'not_found', message: 'introuvable' }, 404)
    const somme = (liste: Op[], sens: Sens) => liste.filter((o) => o.direction === sens).reduce((n, o) => n + o.amount, 0)
    const vueCategorie = (c: Categorie) => ({ ...c, operations: ops.filter((o) => o.category_id === c.id).length, merchants: srv.commercants.filter((m) => m.category_id === c.id).length, rules: srv.regles.filter((r) => r.category_id === c.id).length })
    const vueRegle = (r: Regle) => ({ ...r, position: srv.regles.indexOf(r), operations: ops.filter((o) => o.rule_id === r.id).length })
    const vueCommercant = (m: Commercant, detail = false) => {
      const siennes = ops.filter((o) => o.merchant_id === m.id)
      return { id: m.id, name: m.name, category_id: m.category_id, operations: siennes.length, debits: somme(siennes, 'debit'), credits: somme(siennes, 'credit'), last_date: siennes[0]?.date ?? null, variants: m.keys.length, ...(detail ? { variant_list: [...m.keys].sort().map((key) => ({ key, manual: key !== m.keys[0], operations: ops.filter((o) => o.cle === key).length })) } : {}) }
    }
    const compteur = () => {
      const libres = ops.filter((o) => o.category_id === null)
      return { operations: libres.length, merchants: new Set(libres.map((o) => o.merchant_id)).size, debits: somme(libres, 'debit'), credits: somme(libres, 'credit') }
    }
    if (url.pathname === '/operations') {
      const q = url.searchParams
      const limit = Number(q.get('limit') ?? 50)
      const offset = Number(q.get('offset') ?? 0)
      const filtrees = ops.filter((o) => (!q.get('statement') || o.statement_id === Number(q.get('statement'))) && (!q.get('from') || String(o.date) >= q.get('from')!) && (!q.get('to') || String(o.date) <= q.get('to')!)
        && (!q.get('category') || o.category_id === Number(q.get('category'))) && (!q.get('merchant') || o.merchant_id === Number(q.get('merchant'))) && (q.get('unclassified') !== 'true' || o.category_id === null))
      return json(route, { operations: filtrees.slice(offset, offset + limit), total: filtrees.length, limit, offset })
    }
    const uneOperation = /^\/operations\/(\d+)(\/category)?$/.exec(url.pathname)
    if (uneOperation) {
      const o = ops.find((x) => x.id === Number(uneOperation[1]))
      if (!o) return inconnu()
      if (!uneOperation[2]) return json(route, { operation: o })
      const { category_id, scope } = corps() as { category_id: number | null; scope: string }
      let reclassees = 1
      if (scope === 'merchant') {
        o.exception = false
        const m = srv.commercants.find((c) => c.id === o.merchant_id)!
        m.category_id = category_id
        reclassees = srv.rejouer()
      } else {
        o.exception = category_id !== null
        ;[o.category_id, o.category_source, o.rule_id] = [category_id, category_id === null ? null : 'operation', null]
        srv.rejouer()
      }
      return json(route, { operation: o, reclassified: reclassees })
    }
    if (url.pathname === '/unclassified/count') return json(route, compteur())
    if (url.pathname === '/unclassified') {
      const groupes = new Map<number, Op[]>()
      for (const o of ops) if (o.category_id === null) groupes.set(o.merchant_id!, [...(groupes.get(o.merchant_id!) ?? []), o])
      const groups = [...groupes.entries()].map(([id, liste]) => ({ merchant_id: id, merchant_name: liste[0].merchant_name, operations: liste.length, debits: somme(liste, 'debit'), credits: somme(liste, 'credit'), first_date: liste.at(-1)!.date, last_date: liste[0].date, sample: liste.slice(0, 3) })).sort((a, b) => b.operations - a.operations || a.merchant_id - b.merchant_id)
      return json(route, { ...compteur(), groups })
    }
    if (url.pathname === '/categories' && methode === 'GET') return json(route, { categories: [...srv.categories].sort((a, b) => Number(a.archived) - Number(b.archived) || a.position - b.position).map(vueCategorie) })
    if (url.pathname === '/categories' && methode === 'POST') {
      const c = corps() as { name: string; kind: Nature; monthly_cap: number | null }
      if (srv.categories.some((x) => x.name.toLowerCase() === c.name.toLowerCase())) return json(route, { error: 'conflict', message: 'une catégorie porte déjà ce nom' }, 409)
      const cree: Categorie = { id: srv.suivant++, key: null, name: c.name, kind: c.kind, monthly_cap: c.monthly_cap ?? null, color: srv.categories.length, position: srv.categories.length, archived: false }
      srv.categories.push(cree)
      return json(route, { category: vueCategorie(cree) }, 201)
    }
    const uneCategorie = /^\/categories\/(\d+)$/.exec(url.pathname)
    if (uneCategorie && methode === 'PATCH') {
      const c = srv.categories.find((x) => x.id === Number(uneCategorie[1]))
      if (!c) return inconnu()
      Object.assign(c, corps())
      return json(route, { category: vueCategorie(c) })
    }
    if (url.pathname === '/rules' && methode === 'GET') return json(route, { rules: srv.regles.map(vueRegle) })
    if (url.pathname === '/rules' && methode === 'POST') {
      const r = corps() as { keywords: string[]; category_id: number }
      const cree: Regle = { id: srv.suivant++, key: null, keywords: r.keywords.map(normaliser), type: null, section: null, category_id: r.category_id }
      srv.regles.unshift(cree)
      const reclassees = srv.rejouer()
      return json(route, { rule: vueRegle(cree), reclassified: reclassees }, 201)
    }
    if (url.pathname === '/rules/order' && methode === 'PUT') {
      const { ids } = corps() as { ids: number[] }
      srv.regles = ids.map((id) => srv.regles.find((r) => r.id === id)!)
      const reclassees = srv.rejouer()
      return json(route, { rules: srv.regles.map(vueRegle), reclassified: reclassees })
    }
    const uneRegle = /^\/rules\/(\d+)$/.exec(url.pathname)
    if (uneRegle) {
      const r = srv.regles.find((x) => x.id === Number(uneRegle[1]))
      if (!r) return inconnu()
      if (methode === 'DELETE') {
        srv.regles = srv.regles.filter((x) => x !== r)
        return json(route, { reclassified: srv.rejouer() })
      }
      Object.assign(r, corps())
      const reclassees = srv.rejouer()
      return json(route, { rule: vueRegle(r), reclassified: reclassees })
    }
    if (url.pathname === '/merchants') {
      const q = normaliser(url.searchParams.get('q') ?? '')
      return json(route, { merchants: srv.commercants.filter((m) => !q || m.keys.some((k) => k.includes(q)) || normaliser(m.name).includes(q)).map((m) => vueCommercant(m)).sort((a, b) => b.operations - a.operations || a.name.localeCompare(b.name)) })
    }
    const unCommercant = /^\/merchants\/(\d+)(\/category|\/merge|\/split)?$/.exec(url.pathname)
    if (unCommercant) {
      const m = srv.commercants.find((x) => x.id === Number(unCommercant[1]))
      if (!m) return inconnu()
      if (!unCommercant[2]) {
        if (methode === 'PATCH') m.name = String(corps().name)
        return json(route, { merchant: vueCommercant(m, true) })
      }
      if (unCommercant[2] === '/category') {
        m.category_id = corps().category_id as number | null
        const reclassees = srv.rejouer()
        return json(route, { merchant: vueCommercant(m, true), reclassified: reclassees })
      }
      if (unCommercant[2] === '/merge') {
        const autres = srv.commercants.filter((x) => (corps().merchant_ids as number[]).includes(x.id))
        for (const a of autres) { m.keys.push(...a.keys); m.category_id ??= a.category_id }
        srv.commercants = srv.commercants.filter((x) => !autres.includes(x))
        const reclassees = srv.rejouer()
        return json(route, { merchant: vueCommercant(m, true), reclassified: reclassees })
      }
      const cles = corps().keys as string[]
      m.keys = m.keys.filter((k) => !cles.includes(k))
      const crees = cles.map((k) => { const n: Commercant = { id: srv.suivant++, name: nomDe(k), category_id: m.category_id, keys: [k] }; srv.commercants.push(n); return n })
      srv.rejouer()
      return json(route, { merchant: vueCommercant(m, true), created: crees.map((n) => vueCommercant(n, true)) })
    }
    return json(route, { error: 'not_found', message: 'route inconnue' }, 404)
  })
  return srv
}
