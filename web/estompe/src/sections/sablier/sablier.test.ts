import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { setUnauthorizedHandler } from '../../lib/api'
import { AUJOURDHUI, fauxClient, recaps, statsPleines, statsVides, trophees, usage } from './essais'
import { adminApi, lecturesDe, sablierApi, type Periode } from './sablier'
import { EtatSablier } from './sablier.svelte'

const json = (status: number, body?: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), { status, headers: body === undefined ? {} : { 'Content-Type': 'application/json' } })

let fetchMock: ReturnType<typeof vi.fn>
beforeEach(() => { fetchMock = vi.fn(); vi.stubGlobal('fetch', fetchMock) })
afterEach(() => { vi.unstubAllGlobals(); setUnauthorizedHandler(null) })

describe('client de Sablier', () => {
  it('envoie le battement avec la session, la plateforme et la section', async () => {
    fetchMock.mockResolvedValue(json(204))
    await sablierApi.battre('pwa', 'sillon')
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/heartbeat')
    expect(init.method).toBe('POST')
    expect(init.credentials).toBe('include')
    expect(JSON.parse(init.body)).toEqual({ platform: 'pwa', section: 'sillon' })
  })

  it('lit la période annoncée par /me', async () => {
    fetchMock.mockResolvedValue(json(200, { sub: 'u', heartbeat_seconds: 30 }))
    expect((await sablierApi.me()).heartbeat_seconds).toBe(30)
    expect(fetchMock.mock.calls[0][0]).toBe('/me')
  })

  it('un 401 ne déconnecte pas et n\'est pas retenté', async () => {
    const perdue = vi.fn()
    setUnauthorizedHandler(perdue)
    fetchMock.mockImplementation(async () => json(401, { error: 'unauthorized' }))
    await expect(sablierApi.battre('web', 'messagerie')).rejects.toMatchObject({ status: 401 })
    await expect(sablierApi.me()).rejects.toMatchObject({ status: 401 })
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(perdue).not.toHaveBeenCalled()
  })
})

describe('routes lues par la section', () => {
  it('passe la période et la date aux stats, et vise les bonnes routes', async () => {
    fetchMock.mockImplementation(async () => json(200, {}))
    await sablierApi.usage('week', '2026-11-18')
    await sablierApi.entracte('month', '2026-11-18')
    await sablierApi.sillon('year', '2026-11-18')
    await sablierApi.messagerie('all', '2026-11-18')
    await sablierApi.trophees()
    await sablierApi.series()
    await sablierApi.nonVus()
    await sablierApi.recaps()
    await sablierApi.recap(12)
    await sablierApi.partage()
    await sablierApi.profil('u/autre')
    expect(fetchMock.mock.calls.map((c) => c[0])).toEqual([
      '/usage?period=week&date=2026-11-18', '/entracte?period=month&date=2026-11-18', '/sillon?period=year&date=2026-11-18', '/messaging?period=all&date=2026-11-18',
      '/trophies', '/streaks', '/unseen', '/recaps', '/recaps/12', '/sharing', '/profiles/u%2Fautre',
    ])
  })

  it('C6 : le réglage de partage part en PUT /sharing, le marquage en POST /seen', async () => {
    fetchMock.mockImplementation(async () => json(200, { stats: [], trophies: [] }))
    await sablierApi.partager({ stats: { 'messaging.messages': true } })
    await sablierApi.marquerVus({ trophies: [2] })
    const [put, post] = fetchMock.mock.calls
    expect([put[0], put[1].method, JSON.parse(put[1].body)]).toEqual(['/sharing', 'PUT', { stats: { 'messaging.messages': true } }])
    expect([post[0], post[1].method, JSON.parse(post[1].body)]).toEqual(['/seen', 'POST', { trophies: [2] }])
  })
})

describe('données, journal et administration', () => {
  const appels = () => fetchMock.mock.calls.map((c) => `${c[1].method} ${c[0]}${c[1].body ? ` ${c[1].body}` : ''}`)

  it('C4 et C6 : les effacements envoient le mot EFFACER, pour soi et pour une personne', async () => {
    fetchMock.mockImplementation(async () => json(204))
    await sablierApi.effacer()
    await adminApi.effacer('kc/camille')
    expect(appels()).toEqual(['DELETE /data {"confirm":"EFFACER"}', 'DELETE /admin/people/kc%2Fcamille/data {"confirm":"EFFACER"}'])
  })

  it('lit le journal de ses consultations et rend l’export en fichier nommé', async () => {
    fetchMock.mockResolvedValueOnce(json(200, [{ date: '2026-11-12T20:14:00Z', admin_name: 'Sacha', action: 'consultation' }]))
    expect((await sablierApi.vues())[0].admin_name).toBe('Sacha')
    fetchMock.mockResolvedValueOnce(new Response('{"sub":"kc-moi"}', { status: 200, headers: { 'Content-Type': 'application/json', 'Content-Disposition': 'attachment; filename="sablier-export-2026-11-18.json"' } }))
    const f = await sablierApi.exporter()
    expect(f.nom).toBe('sablier-export-2026-11-18.json')
    expect(await f.blob.text()).toBe('{"sub":"kc-moi"}')
    expect(appels()).toEqual(['GET /views', 'GET /export'])
  })

  it('vise les routes d’administration', async () => {
    fetchMock.mockImplementation(async () => json(200, {}))
    await adminApi.personnes()
    await adminApi.totaux('month', '2026-11-18')
    await adminApi.journal()
    await adminApi.journal(42)
    await adminApi.trophees()
    await adminApi.catalogue()
    await adminApi.creer({ key: 'k', name: 'N', description: 'D', icon: 'film', kind: 'exploit', exploit: 'long_appel' })
    await adminApi.modifier(6, { active: false })
    const admin = lecturesDe('kc camille')
    await admin.usage('week', '2026-11-18')
    await admin.trophees()
    await admin.recap(12)
    expect(appels()).toEqual([
      'GET /admin/people', 'GET /admin/totals?period=month&date=2026-11-18', 'GET /admin/audit?limit=50', 'GET /admin/audit?limit=50&before=42', 'GET /admin/trophies', 'GET /admin/catalogue',
      'POST /admin/trophies {"key":"k","name":"N","description":"D","icon":"film","kind":"exploit","exploit":"long_appel"}', 'PATCH /admin/trophies/6 {"active":false}',
      'GET /admin/people/kc%20camille/usage?period=week&date=2026-11-18', 'GET /admin/people/kc%20camille/trophies', 'GET /admin/people/kc%20camille/recaps/12',
    ])
  })
})

describe('état de la section', () => {
  it('en consultation par un admin : ni annonce, ni non vus, ni marquage', async () => {
    const marquer = vi.fn(async () => {})
    const nonVus = vi.fn(async () => ({ trophies: 3, recaps: 2 }))
    const etat = new EtatSablier(fauxClient({ marquerVus: marquer, nonVus }), () => AUJOURDHUI, true)
    await etat.ouvrir()
    expect(etat.charge).toBe(true)
    expect(etat.trophees?.unlocked.some((t) => !t.seen)).toBe(true)
    expect(etat.annonces).toEqual([])
    expect(etat.recapAnnonce).toBeNull()
    expect(etat.pastille).toBe(false)
    await etat.lireRecap(12)
    await etat.tropheeVu(2)
    expect(marquer).not.toHaveBeenCalled()
    expect(nonVus).not.toHaveBeenCalled()
  })

  const appels = () => {
    const lus: string[] = []
    const noter = <T>(nom: string, rendre: (p: Periode, d: string) => T) => async (p: Periode, d: string) => { lus.push(`${nom} ${p} ${d}`); return rendre(p, d) }
    const s = statsPleines()
    const client = fauxClient({ usage: noter('usage', () => s.usage), entracte: noter('entracte', () => s.entracte), sillon: noter('sillon', () => s.sillon), messagerie: noter('messaging', () => s.messaging) })
    return { lus, client }
  }

  it('C3 : changer de période recharge toutes les stats', async () => {
    const { lus, client } = appels()
    const etat = new EtatSablier(client, () => AUJOURDHUI)
    await etat.ouvrir()
    expect(lus).toEqual(['usage year 2026-11-18', 'usage month 2026-11-18', 'entracte month 2026-11-18', 'sillon month 2026-11-18', 'messaging month 2026-11-18'])
    lus.length = 0
    etat.choisir('week')
    await vi.waitFor(() => expect(lus).toHaveLength(4))
    expect(lus).toEqual(['usage week 2026-11-18', 'entracte week 2026-11-18', 'sillon week 2026-11-18', 'messaging week 2026-11-18'])
    lus.length = 0
    etat.choisir('week')
    expect(lus).toEqual([])
  })

  it('activité des douze derniers mois : l’année en cours, et la précédente si le premier jour la précède', async () => {
    const lus: string[] = []
    const client = (premier: string) => fauxClient({ usage: async (p, d) => { lus.push(`${p} ${d}`); return usage({ seconds: 60, first_day: premier, by_day: [{ day: `${d.slice(0, 4)}-03-01`, seconds: 60 }] }) } })
    const recent = new EtatSablier(client('2026-09-03'), () => AUJOURDHUI)
    await recent.ouvrir()
    expect(lus.filter((l) => l.startsWith('year'))).toEqual(['year 2026-11-18'])
    expect(recent.activite).toEqual([{ day: '2026-03-01', seconds: 60 }])
    lus.length = 0
    const ancien = new EtatSablier(client('2024-03-03'), () => AUJOURDHUI)
    await ancien.ouvrir()
    expect(lus.filter((l) => l.startsWith('year'))).toEqual(['year 2026-11-18', 'year 2025-12-31'])
    expect(ancien.activite?.map((j) => j.day)).toEqual(['2025-03-01', '2026-03-01'])
    const consultation = new EtatSablier(client('2024-03-03'), () => AUJOURDHUI, true)
    lus.length = 0
    await consultation.ouvrir()
    expect(lus.filter((l) => l.startsWith('year'))).toEqual([])
    expect(consultation.activite).toBeNull()
  })

  it('activité illisible : la section s’ouvre sans elle', async () => {
    const etat = new EtatSablier(fauxClient({ usage: async (p) => { if (p === 'year') throw new TypeError('Failed to fetch'); return usage() } }), () => AUJOURDHUI)
    await etat.ouvrir()
    expect(etat.activite).toBeNull()
    expect(etat.charge).toBe(true)
    expect(etat.erreur).toBe('')
  })

  it('C3 : reculer et avancer rechargent, sans dépasser le premier jour ni la période en cours', async () => {
    const { lus, client } = appels()
    const etat = new EtatSablier(client, () => AUJOURDHUI)
    await etat.ouvrir()
    expect(etat.premierJour).toBe('2026-09-03')
    lus.length = 0
    etat.avancer()
    expect(lus).toEqual([])
    etat.reculer()
    etat.reculer()
    expect(etat.date).toBe('2026-09-30')
    etat.reculer()
    expect(etat.date).toBe('2026-09-30')
    await vi.waitFor(() => expect(lus).toHaveLength(8))
    expect(lus[4]).toBe('usage month 2026-09-30')
    etat.avancer()
    etat.avancer()
    etat.avancer()
    expect(etat.date).toBe(AUJOURDHUI)
  })

  it('cherche une période plus large qui a des données pour un domaine vide', async () => {
    const vides = statsVides()
    const pleines = statsPleines()
    const etat = new EtatSablier(fauxClient({
      usage: async () => pleines.usage,
      entracte: async (p) => (p === 'year' ? pleines.entracte : vides.entracte),
      sillon: async () => vides.sillon,
      messagerie: async () => pleines.messaging,
    }), () => AUJOURDHUI)
    etat.periode = 'week'
    await etat.ouvrir()
    await vi.waitFor(() => expect(etat.ailleurs.entracte).toEqual({ periode: 'year', date: AUJOURDHUI }))
    expect(etat.ailleurs.sillon).toBeNull()
    expect(etat.ailleurs.usage).toBeUndefined()
  })

  it('C5 : la pastille suit /unseen et s’éteint quand trophée et récap ont été ouverts', async () => {
    const vus = { trophees: new Set<number>(), recaps: new Set<number>() }
    const marquer = vi.fn(async (v: { trophies?: number[] | 'all'; recaps?: number[] | 'all' }) => {
      if (Array.isArray(v.trophies)) for (const id of v.trophies) vus.trophees.add(id)
      if (Array.isArray(v.recaps)) for (const id of v.recaps) vus.recaps.add(id)
    })
    const etat = new EtatSablier(fauxClient({
      nonVus: async () => ({ trophies: vus.trophees.has(2) ? 0 : 1, recaps: vus.recaps.has(12) ? 0 : 1 }),
      recaps: async () => recaps().map((r) => ({ ...r, seen: r.seen || vus.recaps.has(r.id) })),
      marquerVus: marquer,
    }), () => AUJOURDHUI)
    await etat.chargerNonVus()
    expect(etat.pastille).toBe(true)
    await etat.ouvrir()
    expect(etat.annonces.map((t) => t.id)).toEqual([2])
    expect(etat.recapAnnonce?.id).toBe(12)
    await etat.tropheeVu(2)
    expect(marquer).toHaveBeenCalledWith({ trophies: [2] })
    expect(etat.annonces).toEqual([])
    expect(etat.nonVus).toEqual({ trophies: 0, recaps: 1 })
    expect(etat.pastille).toBe(true)
    await etat.lireRecap(12)
    expect(marquer).toHaveBeenCalledWith({ recaps: [12] })
    await vi.waitFor(() => expect(etat.pastille).toBe(false))
    expect(etat.recapAnnonce).toBeNull()
    expect(etat.recaps?.find((r) => r.id === 12)?.seen).toBe(true)
  })

  it('Sablier injoignable : une erreur à afficher, puis « Réessayer » recharge', async () => {
    let panne = true
    const etat = new EtatSablier(fauxClient({ trophees: async () => { if (panne) throw new TypeError('Failed to fetch'); return trophees() } }), () => AUJOURDHUI)
    await etat.ouvrir()
    expect(etat.erreur).toBe('Les stats sont injoignables pour le moment.')
    expect(etat.charge).toBe(false)
    panne = false
    await etat.ouvrir()
    expect(etat.erreur).toBe('')
    expect(etat.charge).toBe(true)
  })

  it('C6 : partager met à jour les réglages et les trophées affichés', async () => {
    const partager = vi.fn(async () => ({ stats: [], trophies: [{ id: 2, name: 'Encore un épisode', icon: 'tele', shared: true }] }))
    const etat = new EtatSablier(fauxClient({ partager }), () => AUJOURDHUI)
    await etat.ouvrir()
    await etat.partager({ trophies: { 2: true } })
    expect(partager).toHaveBeenCalledWith({ trophies: { 2: true } })
    expect(etat.trophees?.unlocked.find((t) => t.id === 2)?.shared).toBe(true)
    expect(etat.partage?.trophies[0].shared).toBe(true)
  })
})
