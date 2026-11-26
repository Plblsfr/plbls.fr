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

type Mode = 'calendar' | 'statement'
interface PeriodeFausse { id: string; from: string; to: string; days: number; known_through: string; elapsed_days: number; complete: boolean; open: boolean; statement_ids: number[] }
const JOUR_MS = 86_400_000
const ecartJours = (a: string, b: string) => Math.round((Date.parse(b) - Date.parse(a)) / JOUR_MS)
const division = (a: number, b: number) => Math.sign(a) * Math.round(Math.abs(a) / b)
const bp = (part: number, total: number) => (total > 0 ? division(part * 10_000, total) : null)
const ecart = (actuel: number, reference: number) => ({ amount: reference, delta: actuel - reference, delta_bp: bp(actuel - reference, reference) })
const reference = (o: Op) => String(o.purchase_date ?? o.date)

function periodesFausses(srv: Tirelire, mode: Mode): PeriodeFausse[] {
  const releves = srv.releves as { id: number; from: string; to: string }[]
  if (!releves.length) return []
  if (mode === 'statement') return [...releves].sort((a, b) => a.to.localeCompare(b.to)).map((r) => ({ id: r.to, from: r.from, to: r.to, days: Math.max(1, ecartJours(r.from, r.to)), known_through: r.to, elapsed_days: Math.max(1, ecartJours(r.from, r.to)), complete: true, open: false, statement_ids: [r.id] }))
  const fin = releves.map((r) => r.to).sort().at(-1)!
  const mois = [...new Set([...(srv.operations as Op[]).map((o) => reference(o).slice(0, 7)), ...releves.map((r) => r.to.slice(0, 7))])].sort()
  return mois.map((id) => {
    const [a, m] = id.split('-').map(Number)
    const jours = new Date(Date.UTC(a, m, 0)).getUTCDate()
    const to = `${id}-${String(jours).padStart(2, '0')}`
    const open = fin < to && fin >= `${id}-01`
    return { id, from: `${id}-01`, to, days: jours, known_through: open ? fin : to, elapsed_days: open ? Number(fin.slice(8)) : jours, complete: !open, open, statement_ids: releves.filter((r) => r.to.slice(0, 7) === id).map((r) => r.id) }
  })
}

function statistiques(srv: Tirelire, mode: Mode) {
  const periodes = periodesFausses(srv, mode)
  const nature = (o: Op) => srv.categories.find((c) => c.id === o.category_id)?.kind ?? null
  const dans = (p: PeriodeFausse) => (srv.operations as Op[]).filter((o) => (mode === 'calendar' ? reference(o).slice(0, 7) === p.id : p.statement_ids.includes(Number(o.statement_id))))
  const sortie = (o: Op) => (o.direction === 'debit' ? o.amount : -o.amount)
  const depense = (o: Op) => (nature(o) === 'fixe' || nature(o) === 'variable' ? sortie(o) : nature(o) === null && o.direction === 'debit' ? o.amount : 0)
  const agregats = periodes.map((p) => {
    const ops = dans(p)
    const somme = (f: (o: Op) => boolean, v: (o: Op) => number = sortie) => ops.filter(f).reduce((n, o) => n + v(o), 0)
    const fixe = somme((o) => nature(o) === 'fixe')
    const variable = somme((o) => nature(o) === 'variable')
    const aClasser = somme((o) => nature(o) === null && o.direction === 'debit')
    const revenus = -somme((o) => nature(o) === 'revenu') + somme((o) => nature(o) === null && o.direction === 'credit', (o) => o.amount)
    const epargne = somme((o) => nature(o) === 'epargne')
    const parJour = Array.from({ length: p.days }, () => 0)
    for (const o of ops) parJour[Math.min(p.days, Math.max(1, mode === 'calendar' ? Number(reference(o).slice(8)) : ecartJours(p.from, reference(o)))) - 1] += depense(o)
    let total = 0
    return { ops, fixe, variable, aClasser, depenses: fixe + variable + aClasser, revenus, epargne, reste: revenus - fixe - variable - aClasser - epargne, cumul: parJour.map((j) => (total += j)), montant: (id: number) => ops.filter((o) => o.category_id === id).reduce((n, o) => n + sortie(o), 0) }
  })
  const completes = (i: number, n: number) => periodes.map((p, j) => (j < i && p.complete ? j : -1)).filter((j) => j >= 0).slice(-n)
  const moyenne = (i: number, n: number, f: (a: (typeof agregats)[number]) => number) => {
    const js = completes(i, n)
    if (!js.length) return null
    const m = division(js.reduce((s, j) => s + f(agregats[j]), 0), js.length)
    return { ...ecart(f(agregats[i]), m), periods: js.length }
  }
  const avant = (i: number, f: (a: (typeof agregats)[number]) => number) => (i > 0 ? ecart(f(agregats[i]), f(agregats[i - 1])) : null)
  const fenetre = (i: number) => periodes.map((_, j) => j).slice(Math.max(0, i - 11), i + 1)
  const ligne = (i: number, c: Categorie) => {
    const f = (a: (typeof agregats)[number]) => a.montant(c.id)
    const montant = f(agregats[i])
    return { id: c.id, name: c.name, kind: c.kind, color: c.color, archived: c.archived, monthly_cap: c.monthly_cap, amount: montant, operations: agregats[i].ops.filter((o) => o.category_id === c.id).length, share_bp: c.kind === 'fixe' || c.kind === 'variable' ? bp(montant, agregats[i].depenses) : null, previous: avant(i, f), avg3: moyenne(i, 3, f), avg12: moyenne(i, 12, f), cap_delta: c.monthly_cap === null ? null : montant - c.monthly_cap, cap_bp: c.monthly_cap === null ? null : bp(montant, c.monthly_cap), trend: fenetre(i).map((j) => f(agregats[j])) }
  }
  const aligne = (cumul: number[], jours: number) => Array.from({ length: jours }, (_, j) => (j === jours - 1 || j >= cumul.length ? cumul.at(-1)! : cumul[j]))
  const mois = (i: number) => {
    const p = periodes[i]
    const a = agregats[i]
    const lignes = srv.categories.filter((c) => c.kind === 'fixe' || c.kind === 'variable').map((c) => ligne(i, c)).filter((l) => l.amount !== 0 || l.operations > 0 || (!l.archived && (l.monthly_cap !== null || !!l.previous?.amount || !!l.avg3?.amount))).sort((x, y) => y.amount - x.amount || x.name.localeCompare(y.name))
    const depassements = lignes.flatMap((l) => {
      const ref = l.monthly_cap !== null ? { reference: 'cap', montant: l.monthly_cap, periods: null as number | null } : l.avg3 ? { reference: 'average', montant: l.avg3.amount, periods: l.avg3.periods as number | null } : null
      if (!ref) return []
      const exces = l.amount - ref.montant
      const enBp = bp(exces, ref.montant)
      return (ref.reference === 'cap' ? exces >= 100 : exces >= 1000 && (enBp ?? 0) >= 1000) ? [{ category_id: l.id, reference: ref.reference, reference_amount: ref.montant, periods: ref.periods, excess: exces, excess_bp: enBp, rank_amount: 0, rank_percent: 0 }] : []
    }).sort((x, y) => y.excess - x.excess)
    depassements.forEach((d, rang) => (d.rank_amount = rang + 1))
    ;[...depassements].sort((x, y) => (y.excess_bp ?? -1) - (x.excess_bp ?? -1)).forEach((d, rang) => (d.rank_percent = rang + 1))
    const plafonnees = srv.categories.filter((c) => !c.archived && (c.kind === 'fixe' || c.kind === 'variable') && c.monthly_cap !== null)
    const budgetTotal = plafonnees.reduce((n, c) => n + (c.monthly_cap ?? 0), 0)
    const budget = plafonnees.length ? { total: budgetTotal, spent: a.depenses, bp: bp(a.depenses, budgetTotal), delta: a.depenses - budgetTotal, uncapped: a.depenses - plafonnees.reduce((n, c) => n + a.montant(c.id), 0) } : null
    const js = completes(i, 12)
    let projection = null
    let raison: string | null = p.open ? null : 'complete'
    if (p.open && p.elapsed_days < 5) raison = 'too_early'
    else if (p.open) {
      const variable = division((a.variable + a.aClasser) * p.days, p.elapsed_days)
      const fixe = srv.categories.filter((c) => c.kind === 'fixe').reduce((n, c) => n + Math.max(a.montant(c.id), moyenne(i, 3, (x) => x.montant(c.id))?.amount ?? 0), 0)
      projection = { total: variable + fixe, variable, fixed: fixe, fixed_pending: fixe - a.fixe, daily_variable: division(a.variable + a.aClasser, p.elapsed_days), remaining_days: p.days - p.elapsed_days, budget_delta: budget ? variable + fixe - budget.total : null }
    }
    const courant = p.open ? a.cumul.slice(0, p.elapsed_days) : a.cumul
    const precedent = i > 0 ? aligne(agregats[i - 1].cumul, p.days) : null
    return {
      mode, period: p, previous_id: periodes[i - 1]?.id ?? null, next_id: periodes[i + 1]?.id ?? null, periods: periodes.length, operations: a.ops.length,
      expenses: { total: a.depenses, fixed: a.fixe, variable: a.variable, unclassified: a.aClasser, fixed_bp: bp(a.fixe, a.depenses), variable_bp: bp(a.variable, a.depenses), unclassified_bp: bp(a.aClasser, a.depenses), previous: avant(i, (x) => x.depenses), previous_to_date: p.open && precedent ? ecart(a.depenses, precedent[p.elapsed_days - 1]) : null, avg3: moyenne(i, 3, (x) => x.depenses), avg12: moyenne(i, 12, (x) => x.depenses) },
      income: { total: a.revenus, classified: a.revenus, unclassified: 0, unclassified_operations: 0, previous: avant(i, (x) => x.revenus) },
      savings: { total: a.epargne, rate_bp: bp(a.epargne, a.revenus), previous: avant(i, (x) => x.epargne) },
      remaining: { total: a.reste, previous: avant(i, (x) => x.reste) },
      budget, categories: lignes,
      unclassified: { amount: a.aClasser, operations: a.ops.filter((o) => o.category_id === null && o.direction === 'debit').length, share_bp: bp(a.aClasser, a.depenses), previous: avant(i, (x) => x.aClasser) },
      overruns: depassements, overrun_total: depassements.reduce((n, d) => n + d.excess, 0),
      cumulative: { current: courant, previous: precedent ? { id: periodes[i - 1].id, values: precedent } : null, average: js.length ? { periods: js.length, values: Array.from({ length: p.days }, (_, j) => division(js.reduce((s, k) => s + aligne(agregats[k].cumul, p.days)[j], 0), js.length)) } : null },
      projection, projection_reason: raison,
      history: fenetre(i).map((j) => ({ id: periodes[j].id, from: periodes[j].from, to: periodes[j].to, complete: periodes[j].complete, expenses: agregats[j].depenses, income: agregats[j].revenus, savings: agregats[j].epargne, remaining: agregats[j].reste, savings_rate_bp: bp(agregats[j].epargne, agregats[j].revenus) })),
    }
  }
  const detail = (i: number, c: Categorie) => {
    const siennes = (j: number) => agregats[j].ops.filter((o) => o.category_id === c.id)
    const groupes = (j: number) => {
      const res = new Map<number | null, { montant: number; nombre: number }>()
      for (const o of siennes(j)) { const g = res.get(o.merchant_id) ?? { montant: 0, nombre: 0 }; g.montant += sortie(o); g.nombre += 1; res.set(o.merchant_id, g) }
      return res
    }
    const precedents = i > 0 ? groupes(i - 1) : null
    const commercants = [...groupes(i).entries()].map(([id, g]) => ({ merchant_id: id, name: srv.commercants.find((m) => m.id === id)?.name ?? null, amount: g.montant, operations: g.nombre, average: division(g.montant, g.nombre), previous: precedents ? ecart(g.montant, precedents.get(id)?.montant ?? 0) : null })).sort((x, y) => y.amount - x.amount)
    const operations = [...siennes(i)].sort((x, y) => reference(y).localeCompare(reference(x)) || y.id - x.id)
    const historique = fenetre(i).map((j) => ({ id: periodes[j].id, from: periodes[j].from, to: periodes[j].to, complete: periodes[j].complete, amount: agregats[j].montant(c.id) }))
    return { mode, period: periodes[i], previous_id: periodes[i - 1]?.id ?? null, next_id: periodes[i + 1]?.id ?? null, periods: periodes.length, category: ligne(i, c), history: historique, over_cap: c.monthly_cap === null ? 0 : historique.filter((h) => h.amount > c.monthly_cap!).length, merchants: commercants.slice(0, 20), merchants_total: commercants.length, operations: operations.slice(0, 5), operations_total: operations.length }
  }
  return { periodes, agregats, mois, detail, index: (cle: string | null) => (cle ? periodes.findIndex((p) => p.id === cle) : periodes.length - 1) }
}

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
    if (url.pathname === '/operations' || url.pathname === '/operations/export.csv') {
      const q = url.searchParams
      const plat = (t: string) => t.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase()
      const mots = plat(q.get('q') ?? '').split(/\s+/).filter(Boolean)
      const quand = (o: Op) => (q.get('date') === 'purchase' ? reference(o) : String(o.date))
      const filtrees = ops.filter((o) => (!q.get('statement') || o.statement_id === Number(q.get('statement'))) && (!q.get('from') || quand(o) >= q.get('from')!) && (!q.get('to') || quand(o) <= q.get('to')!)
        && (!q.get('category') || o.category_id === Number(q.get('category'))) && (!q.get('merchant') || o.merchant_id === Number(q.get('merchant'))) && (q.get('unclassified') !== 'true' || o.category_id === null)
        && mots.every((m) => plat([o.label, ...(o.details as string[]), o.merchant ?? '', o.merchant_name ?? ''].join('\n')).includes(m))
        && (!q.get('min') || o.amount >= Number(q.get('min'))) && (!q.get('max') || o.amount <= Number(q.get('max'))) && (!q.get('direction') || o.direction === q.get('direction')) && (!q.get('type') || o.type === q.get('type')))
      if (url.pathname === '/operations/export.csv') {
        const fr = (j: unknown) => String(j ?? '').split('-').reverse().join('/')
        const euros = (c: number) => `${c < 0 ? '-' : ''}${Math.trunc(Math.abs(c) / 100)},${String(Math.abs(c) % 100).padStart(2, '0')}`
        const lignes = filtrees.map((o) => [fr(o.date), fr(o.value_date), fr(o.purchase_date), o.label, (o.details as string[]).join(' | '), o.merchant_name ?? '', srv.categories.find((c) => c.id === o.category_id)?.name ?? '', '', o.type, o.section, o.direction === 'debit' ? euros(o.amount) : '', o.direction === 'credit' ? euros(o.amount) : '', euros(o.direction === 'debit' ? -o.amount : o.amount), `Relevé n° ${Number(o.statement_id) - 100} de 2026`].join(';'))
        const dates = filtrees.map((o) => String(o.date)).sort()
        const nom = `tirelire-operations-${q.get('from') ?? dates[0] ?? ''}_${q.get('to') ?? dates.at(-1) ?? ''}.csv`
        return route.fulfill({ status: 200, contentType: 'text/csv; charset=utf-8', body: `\ufeff${['Date;Date de valeur;Date d\'achat;Libellé;Détails;Commerçant;Catégorie;Nature;Type;Section;Débit;Crédit;Montant;Relevé', ...lignes].join('\r\n')}\r\n`, headers: { ...cors, 'access-control-expose-headers': 'Content-Disposition', 'content-disposition': `attachment; filename="${nom}"` } })
      }
      const limit = Number(q.get('limit') ?? 50)
      const offset = Number(q.get('offset') ?? 0)
      return json(route, { operations: filtrees.slice(offset, offset + limit), total: filtrees.length, total_debits: somme(filtrees, 'debit'), total_credits: somme(filtrees, 'credit'), limit, offset })
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
    if (url.pathname.startsWith('/stats/')) {
      const mode: Mode = url.searchParams.get('mode') === 'statement' ? 'statement' : 'calendar'
      const st = statistiques(srv, mode)
      if (url.pathname === '/stats/months') return json(route, { mode, periods: st.periodes.map((p, i) => ({ ...p, operations: st.agregats[i].ops.length, expenses: st.agregats[i].depenses, income: st.agregats[i].revenus, savings: st.agregats[i].epargne, remaining: st.agregats[i].reste })) })
      const i = st.index(url.searchParams.get('period'))
      if (i < 0) return inconnu()
      if (url.pathname === '/stats/month') return json(route, st.mois(i))
      const c = srv.categories.find((x) => x.id === Number(/^\/stats\/month\/categories\/(\d+)$/.exec(url.pathname)?.[1]))
      return c ? json(route, st.detail(i, c)) : inconnu()
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
