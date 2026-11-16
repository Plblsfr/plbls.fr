import { expect, test, type Page, type Route } from '@playwright/test'

const ORIGINE = 'http://localhost:4175'
const ESTOMPE = 'http://estompe.test'
const ENTRACTE = 'http://entracte.test'
const SILLON = 'http://sillon.test'

type Profil = { estompe: boolean; sillon: boolean; entracte?: boolean }

const cors = { 'access-control-allow-origin': ORIGINE, 'access-control-allow-credentials': 'true' }
const json = (route: Route, body: unknown, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body), headers: cors })
const preflight = (route: Route) => route.fulfill({ status: 204, headers: { ...cors, 'access-control-allow-headers': '*', 'access-control-allow-methods': '*' } })

function wav(secondes: number) {
  const taux = 8000
  const n = Math.round(taux * secondes)
  const b = Buffer.alloc(44 + n * 2)
  b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2, 4); b.write('WAVE', 8); b.write('fmt ', 12)
  b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22); b.writeUInt32LE(taux, 24); b.writeUInt32LE(taux * 2, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34)
  b.write('data', 36); b.writeUInt32LE(n * 2, 40)
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.round(Math.sin((2 * Math.PI * 440 * i) / taux) * 3000), 44 + i * 2)
  return b
}

const artiste = { id: 'a0000000-0000-0000-0000-000000000001', name: 'Les Sillons', overview: 'Groupe de test.', genres: ['Pop'], image: false, fanart: false, available: true, album_count: 1 }
const album = { id: 'b0000000-0000-0000-0000-000000000001', title: 'Premier Pressage', type: 'Album', secondary_types: [], release_date: '2024-03-01', year: 2024, genres: ['Pop'], cover: false, available: true, added_at: '2026-11-01T10:00:00Z', track_count: 4, duration: 64, artist: { id: artiste.id, name: artiste.name } }
const DUREES = [3, 1.2, 30, 30]
const pistes = DUREES.map((d, i) => ({
  id: `c0000000-0000-0000-0000-00000000000${i + 1}`, title: `Morceau ${i + 1}`, disc: 1, number: i + 1, duration: d, explicit: false, available: true,
  album: { id: album.id, title: album.title, year: 2024, cover: false }, artist: { id: artiste.id, name: artiste.name }, format: { codec: 'pcm', container: 'wav', bitrate: 128000, sample_rate: 8000, channels: 1, bits: 16 },
}))

async function simuler(page: Page, p: Profil) {
  const appelsEstompe: string[] = []
  const lectures: { id: string; corps: { codecs: string[]; containers: string[]; quality?: string } }[] = []
  await page.route(`${ESTOMPE}/**`, (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() === 'OPTIONS') return preflight(route)
    appelsEstompe.push(url.pathname)
    switch (url.pathname) {
      case '/me':
        return json(route, p.estompe
          ? { activated: true, allowed: true, admin: false, rights: ['access'], id: 'me', pseudo: 'moi', sub: 'kc-moi', cinema: false, blocks: ['vocal'] }
          : { activated: false, allowed: false, admin: false, rights: [] })
      case '/unread': return json(route, { channels: [], dms: [], counts: {} })
      case '/me/notify-settings': return json(route, { levels: {}, types: { dm: true, mention: true, channel: true, voice: true }, preview: true, sound: true })
      case '/avatars': case '/statuses': return json(route, {})
      case '/ws-ticket': return json(route, { ticket: 't' })
      default: return json(route, [])
    }
  })
  await page.route(`${ENTRACTE}/**`, (route) => {
    if (route.request().method() === 'OPTIONS') return preflight(route)
    const url = new URL(route.request().url())
    if (url.pathname === '/me') return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: !!p.entracte, request: false, admin: false }, push: { web: false, fcm: false } })
    if (url.pathname === '/library') return json(route, { items: [], total: 0 })
    if (url.pathname === '/genres') return json(route, { genres: [] })
    return json(route, { items: [], total: 0, last_id: 0 })
  })
  await page.route(`${SILLON}/**`, (route) => {
    const req = route.request()
    const url = new URL(req.url())
    if (req.method() === 'OPTIONS') return preflight(route)
    if (url.pathname.startsWith('/stream/')) {
      const i = pistes.findIndex((x) => x.id === url.pathname.slice(8))
      return route.fulfill({ status: 200, contentType: 'audio/wav', body: wav(DUREES[i]), headers: { 'access-control-allow-origin': '*', 'accept-ranges': 'none' } })
    }
    const playback = /^\/tracks\/([^/]+)\/playback$/.exec(url.pathname)
    if (playback && req.method() === 'POST') {
      lectures.push({ id: playback[1], corps: req.postDataJSON() })
      const i = pistes.findIndex((x) => x.id === playback[1])
      return json(route, {
        track: playback[1], url: `/stream/${playback[1]}`, expires_at: '2026-11-17T00:00:00Z', mode: 'direct', quality: 'originale',
        format: { codec: 'pcm', container: 'wav', mime: 'audio/wav', bitrate: 128000 }, duration: DUREES[i], samples: Math.round(DUREES[i] * 8000), sample_rate: 8000, output_sample_rate: 8000, encoder_delay: 0,
        gain: { reference: -18, loudness: -20, track: 2, track_peak: -6, album: 2, album_peak: -6 },
      })
    }
    switch (url.pathname) {
      case '/me': return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: p.sillon, add: false, admin: false }, sections: p.sillon ? ['library'] : [] })
      case '/preferences': return json(route, { quality: 'originale', qualities: ['originale', 'haute', 'normale', 'basse'] })
      case '/albums/recent': return json(route, { items: [album], next_cursor: null })
      case '/artists': return json(route, { items: [artiste], next_cursor: null })
      case `/artists/${artiste.id}`: return json(route, { ...artiste, albums: [album] })
      case `/albums/${album.id}`: return json(route, { ...album, tracks: pistes })
      case '/search': return json(route, url.searchParams.get('q')?.toLowerCase().includes('morceau') ? { artists: [], albums: [], tracks: pistes } : { artists: [artiste], albums: [album], tracks: [] })
      default: return json(route, { error: 'not_found', message: 'introuvable' }, 404)
    }
  })
  return { appelsEstompe, lectures }
}

const mini = (page: Page) => page.getByRole('group', { name: 'Sillon, lecture en cours' })

test('Sillon seul : la section s’ouvre directement, sans messagerie', async ({ page }) => {
  const s = await simuler(page, { estompe: false, sillon: true })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Sillon', level: 1 })).toBeVisible()
  await expect(page).toHaveURL(/#\/musique/)
  await expect(page.locator('.film-card strong', { hasText: 'Premier Pressage' })).toBeVisible()
  await expect(page.getByRole('navigation', { name: 'Navigation principale' })).toHaveCount(0)
  await expect(page.getByText('Messages privés')).toHaveCount(0)
  expect(s.appelsEstompe.filter((c) => c !== '/me')).toEqual([])
})

test('Estompe sans sillon:access : aucune entrée Sillon, écran « pas d’accès » sur un lien', async ({ page }) => {
  await simuler(page, { estompe: true, sillon: false })
  await page.goto('/')
  await expect(page.getByRole('navigation', { name: 'Navigation principale' })).toBeVisible()
  await expect(page.getByRole('link', { name: /Sillon/ })).toHaveCount(0)
  await page.goto(`/#/musique/album/${album.id}`)
  await expect(page.getByRole('heading', { name: 'Pas d’accès à Sillon' })).toBeVisible()
})

test('Estompe et Sillon : l’album se joue dans l’ordre, la lecture continue dans la messagerie et le mini-lecteur la pilote', async ({ page }) => {
  const s = await simuler(page, { estompe: true, sillon: true })
  await page.goto('/')
  await page.getByRole('link', { name: 'Sillon, musique' }).click()
  await expect(page.getByRole('heading', { name: 'Sillon', level: 1 })).toBeVisible()
  await page.locator('.film-card', { hasText: 'Premier Pressage' }).click()
  await expect(page.getByRole('heading', { name: 'Premier Pressage', level: 1 })).toBeVisible()
  await page.getByRole('button', { name: 'Écouter l’album' }).click()

  await expect(mini(page)).toContainText('Morceau 1')
  await expect(mini(page).getByRole('button', { name: 'Pause' })).toBeVisible()
  await expect.poll(() => s.lectures.map((l) => l.id)).toEqual([pistes[0].id, pistes[1].id])
  await expect(mini(page)).toContainText('Morceau 1')
  expect(s.lectures[0].corps.containers).toContain('wav')
  await expect(mini(page)).toContainText('Morceau 2')
  await expect(mini(page)).toContainText('Morceau 3')
  await expect.poll(() => s.lectures.map((l) => l.id)).toEqual(pistes.map((x) => x.id))

  await page.getByRole('link', { name: 'Messages privés' }).click()
  await expect(page.getByRole('heading', { name: 'Sillon', level: 1 })).toHaveCount(0)
  await expect(mini(page)).toContainText('Morceau 3')
  await expect(mini(page).getByRole('button', { name: 'Pause' })).toBeVisible()
  await mini(page).getByRole('button', { name: 'Pause' }).click()
  await expect(mini(page).getByRole('button', { name: 'Lecture' })).toBeVisible()
  await mini(page).getByRole('button', { name: 'Lecture' }).click()
  await expect(mini(page).getByRole('button', { name: 'Pause' })).toBeVisible()
  await mini(page).getByRole('button', { name: 'Suivant' }).click()
  await expect(mini(page)).toContainText('Morceau 4')
  await expect(mini(page).getByRole('button', { name: 'Pause' })).toBeVisible()
  await mini(page).getByRole('button', { name: 'Arrêter la musique' }).click()
  await expect(mini(page)).toHaveCount(0)
})

test('Navigation : artistes, fiche artiste, recherche', async ({ page }) => {
  await simuler(page, { estompe: false, sillon: true })
  await page.goto('/#/musique')
  await page.getByRole('link', { name: 'Artistes' }).click()
  await page.locator('.film-card', { hasText: 'Les Sillons' }).click()
  await expect(page.getByRole('heading', { name: 'Les Sillons', level: 1 })).toBeVisible()
  await page.getByRole('link', { name: 'Rechercher' }).click()
  await page.getByLabel('Rechercher dans la musique').fill('morceau')
  await page.getByRole('button', { name: 'Lire : Morceau 4, Les Sillons' }).click()
  await expect(mini(page)).toContainText('Morceau 4')
})

test('Lecture automatique refusée : « Activer le son » au lieu d’une erreur', async ({ page }) => {
  await page.addInitScript(() => {
    const jouer = HTMLMediaElement.prototype.play
    let refus = 1
    HTMLMediaElement.prototype.play = function () {
      if (refus-- > 0) return Promise.reject(new DOMException('geste requis', 'NotAllowedError'))
      return jouer.call(this)
    }
  })
  await simuler(page, { estompe: false, sillon: true })
  await page.goto(`/#/musique/album/${album.id}`)
  await page.getByRole('button', { name: 'Écouter l’album' }).click()
  const activer = mini(page).getByRole('button', { name: 'Activer le son' })
  await expect(activer).toBeVisible()
  await expect(mini(page).getByRole('alert')).toHaveCount(0)
  await activer.click()
  await expect(activer).toHaveCount(0)
  await expect(mini(page).getByRole('button', { name: 'Pause' })).toBeVisible()
})

test('Sillon et Entracte sans messagerie : les deux sections', async ({ page }) => {
  await simuler(page, { estompe: false, sillon: true, entracte: true })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Entracte', level: 1 })).toBeVisible()
  await page.getByRole('link', { name: 'Sillon · musique' }).click()
  await expect(page.getByRole('heading', { name: 'Sillon', level: 1 })).toBeVisible()
})
