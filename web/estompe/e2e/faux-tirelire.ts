import type { Page, Route } from '@playwright/test'
import { simuler, type Profil } from './faux-sablier'

export const TIRELIRE = 'http://tirelire.test'
export const PORT = 4179

type Sens = 'debit' | 'credit'
interface Ligne { jour: string; type: string; section: string; label: string; details: string[]; merchant: string | null; achat: string | null; amount: number; direction: Sens }

const COMMERCES = ['MARCHE COLLINE VILLENEUVE', 'CAFE TAMIS VILLENEUVE', 'BISTROT LUNE SAINT-ELOI', 'CINEMA LE PHARE VILLENEUVE', 'LIBRAIRIE DES QUAIS SAINT-ELOI', 'BOULANGERIE DU PONT VILLENEUVE']
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

export function serveurTirelire(avecReleves = false) {
  const srv = {
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
      for (const o of ops) srv.operations.push({ id: srv.prochain++, statement_id: id, account_id: 1, date: o.jour, value_date: o.jour, purchase_date: o.achat, section: o.section, type: o.type, label: o.label, details: o.details, merchant: o.merchant, amount: o.amount, direction: o.direction, merchant_id: null, category_id: null })
      srv.operations.sort((a, b) => String(b.date).localeCompare(String(a.date)) || Number(a.id) - Number(b.id))
      srv.fichiers.set(fichier, id)
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
    if (url.pathname === '/operations') {
      const q = url.searchParams
      const limit = Number(q.get('limit') ?? 50)
      const offset = Number(q.get('offset') ?? 0)
      const filtrees = srv.operations.filter((o) => (!q.get('statement') || o.statement_id === Number(q.get('statement'))) && (!q.get('from') || String(o.date) >= q.get('from')!) && (!q.get('to') || String(o.date) <= q.get('to')!))
      return json(route, { operations: filtrees.slice(offset, offset + limit), total: filtrees.length, limit, offset })
    }
    return json(route, { error: 'not_found', message: 'route inconnue' }, 404)
  })
  return srv
}
