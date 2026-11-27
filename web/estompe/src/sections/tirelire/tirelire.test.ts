import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, setUnauthorizedHandler } from '../../lib/api'
import { categorie, CATEGORIES, dejaImporte, fauxClient, file, groupe, importe, operation, OPERATIONS, page, pdf, refuse, regle, releve } from './essais'
import { actives, bilan, budget, conditionDe, deplacer, ENVOI_MAX, FICHIER_MAX, filtres, lots, motCle, teinte, messageErreur, messageRefus, natureDe, parGravite, parJour, raisonControle, requeteOperations, tirelireApi, titreDe, totaux, totauxConnus, type Filtres, type Issue } from './tirelire'
import { EtatTirelire } from './tirelire.svelte'

const json = (status: number, body?: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), { status, headers: body === undefined ? {} : { 'Content-Type': 'application/json' } })
const TOUT: Filtres = filtres()

let fetchMock: ReturnType<typeof vi.fn>
beforeEach(() => { fetchMock = vi.fn(); vi.stubGlobal('fetch', fetchMock) })
afterEach(() => { vi.unstubAllGlobals(); setUnauthorizedHandler(null) })

describe('client de Tirelire', () => {
  it('lit /me avec la session, sans déconnecter sur un 401', async () => {
    const perdue = vi.fn()
    setUnauthorizedHandler(perdue)
    fetchMock.mockImplementation(async () => json(401, { error: 'unauthorized', message: 'connexion requise' }))
    await expect(tirelireApi.me()).rejects.toMatchObject({ status: 401 })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock.mock.calls[0][0]).toBe('/me')
    expect(fetchMock.mock.calls[0][1].credentials).toBe('include')
    expect(perdue).not.toHaveBeenCalled()
  })

  it('rend les comptes et les relevés', async () => {
    fetchMock.mockResolvedValueOnce(json(200, { accounts: [{ id: 1, bank: 'cmb', iban: 'FR00 0000 0000 0000 0000 0000 000', bic: null, number: null, reference: null, holder: 'Camille Exemple', statements: 2 }] }))
    expect((await tirelireApi.comptes())[0].statements).toBe(2)
    expect(fetchMock.mock.calls[0][0]).toBe('/accounts')
    fetchMock.mockResolvedValueOnce(json(200, { statements: [releve()] }))
    expect((await tirelireApi.releves())[0].id).toBe(11)
    expect(fetchMock.mock.calls[1][0]).toBe('/statements')
  })

  it('envoie chaque PDF dans un champ file du même envoi', async () => {
    fetchMock.mockResolvedValue(json(200, { results: [importe('a.pdf'), dejaImporte('b.pdf')] }))
    const resultats = await tirelireApi.importer([pdf('a.pdf'), pdf('b.pdf')])
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/statements/import')
    expect(init.method).toBe('POST')
    expect(init.body).toBeInstanceOf(FormData)
    expect((init.body as FormData).getAll('file').map((f) => (f as File).name)).toEqual(['a.pdf', 'b.pdf'])
    expect(init.headers['Content-Type']).toBeUndefined()
    expect(resultats.map((r) => r.status)).toEqual(['imported', 'already_imported'])
  })

  it('supprime un relevé', async () => {
    fetchMock.mockResolvedValue(json(204))
    await tirelireApi.supprimer(11)
    expect(fetchMock.mock.calls[0][0]).toBe('/statements/11')
    expect(fetchMock.mock.calls[0][1].method).toBe('DELETE')
  })

  it('passe les filtres et la pagination aux opérations', async () => {
    expect(requeteOperations(TOUT)).toBe('/operations?limit=50&offset=0')
    expect(requeteOperations(filtres({ releve: 11, du: '2026-11-01', au: '2026-11-26' }), 50)).toBe('/operations?statement=11&from=2026-11-01&to=2026-11-26&limit=50&offset=50')
    fetchMock.mockResolvedValue(json(200, page()))
    const p = await tirelireApi.operations(filtres({ releve: 11, du: '', au: '' }), 100)
    expect(fetchMock.mock.calls[0][0]).toBe('/operations?statement=11&limit=50&offset=100')
    expect(p.total).toBe(5)
  })

  it('passe les filtres de catégorie, de commerçant et « à classer »', () => {
    expect(requeteOperations(filtres({ categorie: 4, commercant: 9, aClasser: true }), 0, 10)).toBe('/operations?category=4&merchant=9&unclassified=true&limit=10&offset=0')
  })

  it('appelle les routes des catégories, des règles, des commerçants et du classement', async () => {
    fetchMock.mockImplementation(async () => json(200, { categories: CATEGORIES, category: categorie(), rules: [regle()], rule: regle(), merchants: [], merchant: {}, operation: operation(), created: [], reclassified: 3, operations: 2, merchants_count: 0, groups: [] }))
    expect(await tirelireApi.categories()).toHaveLength(6)
    await tirelireApi.creerCategorie({ name: 'Animaux', kind: 'variable', monthly_cap: 4000 })
    await tirelireApi.modifierCategorie(3, { monthly_cap: null, archived: true })
    await tirelireApi.regles()
    await tirelireApi.creerRegle({ keywords: ['LIVREO'], category_id: 3 })
    await tirelireApi.modifierRegle(5, { category_id: 2 })
    await tirelireApi.supprimerRegle(5)
    await tirelireApi.ordonnerRegles([2, 1])
    await tirelireApi.commercants('café & co')
    await tirelireApi.commercant(7)
    await tirelireApi.classerCommercant(7, null)
    await tirelireApi.fusionner(7, [8, 9])
    await tirelireApi.separer(7, ['CAFE TAMIS VILLENEUVE'])
    expect((await tirelireApi.operation(4)).id).toBe(1)
    expect((await tirelireApi.classerOperation(4, 2, 'merchant')).reclassified).toBe(3)
    await tirelireApi.aClasser()
    await tirelireApi.compteur()
    const appels = fetchMock.mock.calls.map((c) => [c[1].method, c[0], c[1].body ? JSON.parse(c[1].body as string) : undefined])
    expect(appels).toEqual([
      ['GET', '/categories', undefined],
      ['POST', '/categories', { name: 'Animaux', kind: 'variable', monthly_cap: 4000 }],
      ['PATCH', '/categories/3', { monthly_cap: null, archived: true }],
      ['GET', '/rules', undefined],
      ['POST', '/rules', { keywords: ['LIVREO'], category_id: 3 }],
      ['PATCH', '/rules/5', { category_id: 2 }],
      ['DELETE', '/rules/5', undefined],
      ['PUT', '/rules/order', { ids: [2, 1] }],
      ['GET', '/merchants?q=caf%C3%A9%20%26%20co', undefined],
      ['GET', '/merchants/7', undefined],
      ['PUT', '/merchants/7/category', { category_id: null }],
      ['POST', '/merchants/7/merge', { merchant_ids: [8, 9] }],
      ['POST', '/merchants/7/split', { keys: ['CAFE TAMIS VILLENEUVE'] }],
      ['GET', '/operations/4', undefined],
      ['PUT', '/operations/4/category', { category_id: 2, scope: 'merchant' }],
      ['GET', '/unclassified', undefined],
      ['GET', '/unclassified/count', undefined],
    ])
  })

  it('un 401 sur une route de la section prévient l’app après une seconde tentative', async () => {
    const perdue = vi.fn()
    setUnauthorizedHandler(perdue)
    fetchMock.mockImplementation(async () => json(401, { error: 'unauthorized', message: 'connexion requise' }))
    await expect(tirelireApi.releves()).rejects.toMatchObject({ status: 401, code: 'unauthorized' })
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(perdue).toHaveBeenCalledTimes(1)
  })

  it('rend le code et le statut d’un refus', async () => {
    fetchMock.mockImplementation(async () => json(403, { error: 'forbidden', message: 'permission tirelire:access manquante' }))
    await expect(tirelireApi.releves()).rejects.toMatchObject({ status: 403, code: 'forbidden' })
    fetchMock.mockImplementation(async () => json(413, { error: 'too_large', message: 'envoi trop volumineux' }))
    await expect(tirelireApi.importer([pdf('a.pdf')])).rejects.toMatchObject({ status: 413, code: 'too_large' })
  })
})

describe('messages d’erreur', () => {
  it('distingue session, droit, taille, panne et réseau', () => {
    expect(messageErreur(new ApiError(401, 'unauthorized', ''))).toMatch(/session a expiré/)
    expect(messageErreur(new ApiError(403, 'forbidden', ''))).toBe('Tu n’as pas accès à Tirelire.')
    expect(messageErreur(new ApiError(403, 'bad_origin', ''))).toMatch(/pas autorisée/)
    expect(messageErreur(new ApiError(413, 'too_large', ''))).toMatch(/8 Mo au plus par fichier, 32 Mo par envoi/)
    expect(messageErreur(new ApiError(400, 'invalid', ''))).toMatch(/pas pu lire/)
    expect(messageErreur(new ApiError(404, 'not_found', ''))).toMatch(/n’existe plus/)
    expect(messageErreur(new ApiError(0, 'timeout', ''))).toMatch(/ne répond pas/)
    expect(messageErreur(new ApiError(503, 'aegis_unavailable', ''))).toMatch(/indisponible/)
    expect(messageErreur(new ApiError(502, 'gateway_unavailable', ''))).toMatch(/indisponible/)
    expect(messageErreur(new TypeError('Failed to fetch'))).toMatch(/injoignable/)
  })

  it('traduit les refus d’un fichier et garde le message d’un code inconnu', () => {
    expect(messageRefus({ code: 'not_a_pdf', message: 'x' })).toBe('Ce fichier n’est pas un PDF.')
    expect(messageRefus({ code: 'too_large', message: 'x' })).toBe('Ce fichier dépasse 8 Mo.')
    expect(messageRefus({ code: 'unknown_format', message: 'x' })).toMatch(/Crédit Mutuel de Bretagne/)
    expect(messageRefus({ code: 'nouveau_code', message: 'message du serveur' })).toBe('message du serveur')
  })

  it('dit ce que le contrôle reproche au relevé', () => {
    expect(raisonControle({ code: 'checks_failed', message: '', gaps: [{ check: 'total_debits', expected: 2, read: 1 }] })).toBe('les totaux lus ne correspondent pas aux totaux imprimés sur le relevé.')
    expect(raisonControle({ code: 'checks_failed', message: '', unrecognized_lines: [{ page: 1, text: 'x' }] })).toBe('une ligne du relevé n’a pas été reconnue.')
    expect(raisonControle({ code: 'checks_failed', message: '', gaps: [{ check: 'total_debits', expected: 2, read: 1 }], unrecognized_lines: [{ page: 1, text: 'x' }, { page: 1, text: 'y' }], missing: ['IBAN'] }))
      .toBe('les totaux lus ne correspondent pas aux totaux imprimés sur le relevé, des lignes du relevé n’ont pas été reconnues et des éléments du relevé sont introuvables.')
    expect(raisonControle({ code: 'checks_failed', message: '' })).toBe('le relevé ne passe pas les vérifications.')
  })
})

describe('calculs', () => {
  it('groupe les opérations par jour, dans l’ordre reçu, avec le solde du jour en centimes', () => {
    const jours = parJour(OPERATIONS)
    expect(jours.map((j) => [j.jour, j.operations.map((o) => o.id), j.solde])).toEqual([
      ['2026-11-26', [1, 2], -3510],
      ['2026-11-24', [3, 4], -29_180],
      ['2026-11-23', [5], 264_000],
    ])
    expect(parJour([])).toEqual([])
  })

  it('additionne débits et crédits sans flottant', () => {
    expect(totaux(OPERATIONS)).toEqual({ debits: 32_690, credits: 264_000 })
    expect(totaux([operation({ amount: 10 }), operation({ amount: 20 }), operation({ amount: 30 })])).toEqual({ debits: 60, credits: 0 })
  })

  it('ne donne des totaux que s’ils sont exacts', () => {
    const releves = [releve()]
    expect(totauxConnus(TOUT, releves, OPERATIONS, 5)).toEqual({ debits: 32_690, credits: 264_000 })
    expect(totauxConnus(TOUT, releves, OPERATIONS, 84)).toBeNull()
    expect(totauxConnus(filtres({ releve: 11, du: '', au: '' }), releves, OPERATIONS, 84)).toEqual({ debits: 246_340, credits: 284_000 })
    expect(totauxConnus(filtres({ releve: 11, du: '2026-11-10', au: '' }), releves, OPERATIONS, 84)).toBeNull()
    expect(totauxConnus(filtres({ releve: 99, du: '', au: '' }), releves, OPERATIONS, 84)).toBeNull()
  })

  it('nomme une opération par son commerçant, sinon par son libellé', () => {
    expect(titreDe(OPERATIONS[0])).toBe('MARCHE COLLINE VILLENEUVE')
    expect(titreDe(OPERATIONS[3])).toBe('VIR VERS LIVRET')
    expect(natureDe(OPERATIONS[0])).toBe('débit · carte')
    expect(natureDe(OPERATIONS[4])).toBe('crédit · virement')
  })

  it('teinte, budget, mots-clés et ordre des règles', () => {
    expect([teinte(categorie({ color: 0 })), teinte(categorie({ color: 7 })), teinte(categorie({ color: 8 })), teinte(null)]).toEqual([1, 8, 0, 0])
    expect(actives(CATEGORIES).map((c) => c.name)).toEqual(['Logement', 'Courses', 'Restaurants', 'Épargne', 'Revenus'])
    expect(budget([...CATEGORIES, categorie({ id: 9, kind: 'epargne', monthly_cap: 99_999 }), categorie({ id: 10, monthly_cap: 500, archived: true })])).toBe(131_000)
    expect([motCle(' Marché  colline* '), motCle('l’Épicerie'), motCle('--')]).toEqual(['MARCHE COLLINE', 'L EPICERIE', ''])
    expect(conditionDe(regle({ type: 'echeance_pret' }))).toBe('type « échéance de prêt »')
    expect(conditionDe(regle({ type: 'cotisation', section: 'SERVICES ET FRAIS BANCAIRES' }))).toBe('type « cotisation » · section « SERVICES ET FRAIS BANCAIRES »')
    expect(conditionDe(regle())).toBe('')
    expect([deplacer([1, 2, 3], 2, 0), deplacer([1, 2, 3], 0, 1), deplacer([1, 2, 3], 0, 3), deplacer([1, 2, 3], 1, 1)]).toEqual([[3, 1, 2], [2, 1, 3], [1, 2, 3], [1, 2, 3]])
  })

  it('découpe un envoi : fichiers trop gros écartés, 24 fichiers et 32 Mo au plus par lot', () => {
    const petits = Array.from({ length: 30 }, (_, i) => pdf(`r${i}.pdf`))
    expect(lots(petits).lots.map((l) => l.length)).toEqual([24, 6])
    const gros = pdf('gros.pdf', FICHIER_MAX + 1)
    const limite = pdf('limite.pdf', FICHIER_MAX)
    expect(lots([gros, limite])).toEqual({ lots: [[limite]], tropGros: [gros] })
    const lourds = Array.from({ length: 5 }, (_, i) => pdf(`l${i}.pdf`, 7 * 1024 * 1024))
    const plan = lots(lourds)
    expect(plan.lots.map((l) => l.length)).toEqual([4, 1])
    for (const l of plan.lots) expect(l.reduce((n, f) => n + f.size, 0)).toBeLessThan(ENVOI_MAX)
    expect(lots([])).toEqual({ lots: [], tropGros: [] })
  })

  it('fait le bilan d’un import et met les refus en tête', () => {
    const issues: Issue[] = [importe('a.pdf'), dejaImporte('b.pdf'), refuse('c.pdf'), { file: 'd.pdf', status: 'failed', message: 'x' }, importe('e.pdf', { id: 12, operations: 16 })]
    expect(bilan(issues)).toEqual({ importes: 2, deja: 1, refuses: 2, operations: 100 })
    expect(parGravite(issues).map((i) => i.file)).toEqual(['c.pdf', 'd.pdf', 'a.pdf', 'e.pdf', 'b.pdf'])
  })
})

describe('état de la section', () => {
  it('charge les relevés une fois, et dit pourquoi en cas d’échec', async () => {
    const releves = vi.fn(async () => [releve()])
    const etat = new EtatTirelire(fauxClient({ releves }))
    await etat.ouvrir()
    await etat.ouvrir()
    expect(releves).toHaveBeenCalledTimes(1)
    expect(etat.releves).toHaveLength(1)
    const panne = new EtatTirelire(fauxClient({ releves: async () => { throw new TypeError('Failed to fetch') } }))
    await panne.ouvrir()
    expect(panne.releves).toBeNull()
    expect(panne.erreur).toMatch(/injoignable/)
  })

  it('pagine : la suite s’ajoute, sans doublon, jusqu’au total', async () => {
    const tout = Array.from({ length: 120 }, (_, i) => operation({ id: i + 1 }))
    const operations = vi.fn(async (_f: Filtres, offset = 0) => page(tout.slice(offset, offset + 50), 120, offset))
    const etat = new EtatTirelire(fauxClient({ operations }))
    await etat.chargerOperations()
    expect(etat.operations).toHaveLength(50)
    expect(etat.reste).toBe(70)
    await etat.suite()
    await etat.suite()
    expect(etat.operations).toHaveLength(120)
    expect(etat.reste).toBe(0)
    await etat.suite()
    expect(operations).toHaveBeenCalledTimes(3)
    expect(operations.mock.calls.map((c) => c[1])).toEqual([0, 50, 100])
  })

  it('un filtre repart de la première page ; une réponse dépassée est ignorée', async () => {
    const attentes: ((p: ReturnType<typeof page>) => void)[] = []
    const operations = vi.fn((_f: Filtres) => new Promise<ReturnType<typeof page>>((ok) => attentes.push(ok)))
    const etat = new EtatTirelire(fauxClient({ operations }))
    const premiere = etat.chargerOperations()
    const seconde = etat.filtrer({ releve: 11 })
    attentes[1](page([operation({ id: 7 })], 1))
    attentes[0](page(OPERATIONS))
    await Promise.all([premiere, seconde])
    expect(etat.operations?.map((o) => o.id)).toEqual([7])
    expect(etat.filtre).toBe(true)
    expect(operations.mock.calls[1][0]).toEqual(filtres({ releve: 11, du: '', au: '' }))
  })

  it('une panne des opérations laisse un message et permet de relancer', async () => {
    let panne = true
    const etat = new EtatTirelire(fauxClient({ operations: async () => { if (panne) throw new ApiError(503, 'aegis_unavailable', ''); return page() } }))
    await etat.chargerOperations()
    expect(etat.operations).toBeNull()
    expect(etat.erreurOperations).toMatch(/indisponible/)
    panne = false
    await etat.chargerOperations()
    expect(etat.erreurOperations).toBe('')
    expect(etat.operations).toHaveLength(5)
  })

  it('importe par lots, écarte les fichiers trop gros sans les envoyer et garde l’ordre des fichiers', async () => {
    const importer = vi.fn(async (fichiers: File[]) => fichiers.map((f) => importe(f.name)))
    const releves = vi.fn(async () => [releve()])
    const etat = new EtatTirelire(fauxClient({ importer, releves }))
    const gros = pdf('gros.pdf', FICHIER_MAX + 1)
    await etat.importer([pdf('a.pdf'), gros, pdf('b.pdf')])
    expect(importer).toHaveBeenCalledTimes(1)
    expect(importer.mock.calls[0][0].map((f) => f.name)).toEqual(['a.pdf', 'b.pdf'])
    expect(etat.issues.map((i) => [i.file, i.status])).toEqual([['a.pdf', 'imported'], ['gros.pdf', 'rejected'], ['b.pdf', 'imported']])
    expect(etat.issues[1]).toMatchObject({ error: { code: 'too_large' } })
    expect(etat.peutReessayer(etat.issues[1])).toBe(false)
    expect(releves).toHaveBeenCalledTimes(1)
    expect(etat.envoi).toBe(false)
  })

  it('un envoi qui échoue marque ses fichiers « non envoyés », et la relance les remplace', async () => {
    let panne = true
    const importer = vi.fn(async (fichiers: File[]) => { if (panne) throw new ApiError(413, 'too_large', ''); return fichiers.map((f) => importe(f.name)) })
    const etat = new EtatTirelire(fauxClient({ importer, releves: async () => [releve()] }))
    await etat.importer([pdf('a.pdf')])
    expect(etat.issues).toEqual([{ file: 'a.pdf', status: 'failed', message: expect.stringMatching(/trop volumineux/) }])
    expect(etat.releves).toBeNull()
    panne = false
    expect(etat.peutReessayer(etat.issues[0])).toBe(true)
    await etat.reessayer(etat.issues[0])
    expect(etat.issues.map((i) => i.status)).toEqual(['imported'])
    expect(etat.releves).toHaveLength(1)
  })

  it('supprime un relevé : liste relue, filtre et opérations oubliés ; un échec est dit', async () => {
    const liste = [releve(), releve({ id: 12, number: 10 })]
    const supprimer = vi.fn(async (id: number) => { liste.splice(liste.findIndex((r) => r.id === id), 1) })
    const etat = new EtatTirelire(fauxClient({ supprimer, releves: async () => [...liste], operations: async () => page() }))
    await etat.ouvrir()
    await etat.filtrer({ releve: 11 })
    expect(await etat.supprimer(11)).toBe(true)
    expect(etat.releves?.map((r) => r.id)).toEqual([12])
    expect(etat.filtres.releve).toBeNull()
    expect(etat.operations).toBeNull()
    const refus = new EtatTirelire(fauxClient({ supprimer: async () => { throw new ApiError(404, 'not_found', '') } }))
    expect(await refus.supprimer(3)).toBe(false)
    expect(refus.erreurSuppression).toMatch(/n’existe plus/)
  })

  it('charge les catégories et le compteur à l’ouverture ; un import les relit', async () => {
    let n = 0
    const compteur = vi.fn(async () => ({ operations: (n += 4), merchants: 2, debits: 900, credits: 0 }))
    const etat = new EtatTirelire(fauxClient({ compteur, importer: async (f) => f.map((x) => importe(x.name)) }))
    await etat.ouvrir()
    expect([etat.categories?.length, etat.aClasser, etat.categorie(2)?.name, etat.categorie(99), etat.categorie(null)]).toEqual([6, 4, 'Courses', null, null])
    await etat.importer([pdf('a.pdf')])
    expect(etat.aClasser).toBe(8)
  })

  it('classe un groupe : règle pour le commerçant, confirmation, annulation, et « cette opération seulement »', async () => {
    const groupes = [groupe(), groupe({ merchant_id: null, merchant_name: null, operations: 1, sample: [operation({ id: 9, label: 'RETRAIT DAB' })] })]
    const classerCommercant = vi.fn(async (_id: number, c: number | null) => { if (c !== null) groupes.shift(); else groupes.unshift(groupe()); return { merchant: {} as never, reclassified: 2 } })
    const classerOperation = vi.fn(async () => ({ operation: operation(), reclassified: 1 }))
    const etat = new EtatTirelire(fauxClient({ aClasser: async () => file([...groupes]), classerCommercant, classerOperation }))
    await etat.chargerReference()
    await etat.chargerFile()
    expect([etat.aClasser, etat.groupes.length]).toEqual([3, 2])
    etat.passer(etat.groupes[0])
    expect(etat.groupes.map((g) => g.merchant_id)).toEqual([null, 7])
    etat.choisir(etat.groupes[1])
    expect(etat.groupes[0].merchant_id).toBe(7)
    expect(await etat.classerGroupe(etat.groupes[0], CATEGORIES[2], true)).toBe(true)
    expect(classerCommercant).toHaveBeenLastCalledWith(7, 3)
    expect(etat.dernier).toEqual({ titre: 'Règle créée pour tout le commerçant', detail: '« Cafe Tamis Villeneuve » → Restaurants · 2 opérations reclassées', commercant: 7, operation: null })
    expect([etat.aClasser, etat.classeesIci]).toEqual([1, 2])
    await etat.annuler()
    expect(classerCommercant).toHaveBeenLastCalledWith(7, null)
    expect([etat.dernier, etat.aClasser, etat.classeesIci]).toEqual([null, 3, 0])
    await etat.classerGroupe(etat.groupes[0], CATEGORIES[2], false)
    expect(classerOperation).toHaveBeenLastCalledWith(2, 3, 'operation')
    expect(etat.dernier?.titre).toBe('Opération classée')
    await etat.annuler()
    expect(classerOperation).toHaveBeenLastCalledWith(2, null, 'operation')
  })

  it('un refus du classement laisse un message et ne confirme rien', async () => {
    const etat = new EtatTirelire(fauxClient({ aClasser: async () => file(), classerCommercant: async () => { throw new ApiError(400, 'invalid', '') }, creerCategorie: async () => { throw new ApiError(409, 'conflict', '') } }))
    await etat.chargerFile()
    expect(await etat.classerGroupe(etat.groupes[0], CATEGORIES[1], true)).toBe(false)
    expect([etat.dernier, etat.erreurClassement, etat.occupe]).toEqual([null, 'Tirelire n’a pas pu lire la demande.', false])
    expect(await etat.enregistrerCategorie(null, { name: 'Courses', kind: 'variable', monthly_cap: null })).toBe(false)
    expect(etat.erreurClassement).toBe('Une catégorie porte déjà ce nom.')
  })

  it('lit une opération seule quand elle n’est pas dans la liste chargée', async () => {
    const lire = vi.fn(async (id: number) => { if (id === 404) throw new ApiError(404, 'not_found', ''); return operation({ id }) })
    const etat = new EtatTirelire(fauxClient({ operation: lire, operations: async () => page() }))
    await etat.chargerDetail(77)
    expect([etat.operation(77)?.id, etat.detailAbsent]).toEqual([77, false])
    await etat.chargerOperations()
    await etat.chargerDetail(3)
    expect(lire).toHaveBeenCalledTimes(1)
    expect(etat.operation(3)?.label).toBe('CARTE 22/11 LIVREO*CMD PARIS')
    await etat.chargerDetail(404)
    expect([etat.operation(404), etat.detailAbsent]).toEqual([null, true])
  })
})
