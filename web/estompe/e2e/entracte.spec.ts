import { expect, test, type Page, type Route } from '@playwright/test'

// Trois profils : Entracte seul, Estompe seul, les deux. Les deux API sont simulées ; on vérifie ce que l'app affiche
// et qu'aucun appel à l'API d'Estompe ne renvoie d'erreur à une personne qui n'a qu'Entracte.

const ESTOMPE = 'http://estompe.test'
const ENTRACTE = 'http://entracte.test'

const film = { id: '11111111-1111-1111-1111-111111111111', kind: 'movie', title: 'Le Film du soir', year: 2021, overview: 'Un film.', duration: 5400, genres: ['Drame'], season: null, episode: null, series_id: null, series_title: null, poster: false, backdrop: false, progress: null, available: true }

const pistes = {
  audio: [{ n: 0, label: 'English (EAC3, 5.1)', language: 'eng' }, { n: 1, label: 'VFQ (AC3, 5.1)', language: 'fre' }, { n: 2, label: 'VFF (AC3, 5.1)', language: 'fre' }],
  subtitles: [{ n: 0, label: 'English', language: 'eng', forced: false }, { n: 1, label: 'Français', language: 'fre', forced: false }, { n: 2, label: 'Français forcés', language: 'fre', forced: true }],
}

type Profil = { estompe: boolean; entracte: boolean }

const json = (route: Route, body: unknown, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body), headers: { 'access-control-allow-origin': 'http://localhost:4174', 'access-control-allow-credentials': 'true' } })

async function simuler(page: Page, p: Profil) {
  const erreursEstompe: string[] = []
  const appelsEstompe: string[] = []
  const ouvertures: { audio: number }[] = []
  await page.route(`${ESTOMPE}/**`, (route) => {
    const url = new URL(route.request().url())
    appelsEstompe.push(url.pathname)
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: { 'access-control-allow-origin': 'http://localhost:4174', 'access-control-allow-credentials': 'true', 'access-control-allow-headers': '*', 'access-control-allow-methods': '*' } })
    switch (url.pathname) {
      case '/me':
        return json(route, p.estompe
          ? { activated: true, allowed: true, admin: false, rights: ['access'], id: 'me', pseudo: 'moi', sub: 'kc-moi', cinema: p.entracte, blocks: ['vocal', 'regarder-ensemble'] }
          : { activated: false, allowed: false, admin: false, rights: [] })
      case '/unread': return json(route, { channels: [], dms: [], counts: {} })
      case '/me/notify-settings': return json(route, { levels: {}, types: { dm: true, mention: true, channel: true, voice: true }, preview: true, sound: true })
      case '/avatars': case '/statuses': return json(route, {})
      case '/ws-ticket': return json(route, { ticket: 't' })
      default:
        if (!p.estompe) erreursEstompe.push(url.pathname)
        return json(route, [])
    }
  })
  await page.route(`${ENTRACTE}/**`, (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: { 'access-control-allow-origin': 'http://localhost:4174', 'access-control-allow-credentials': 'true', 'access-control-allow-headers': '*', 'access-control-allow-methods': '*' } })
    switch (url.pathname) {
      case '/me': return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: p.entracte, request: p.entracte, admin: false }, push: { web: false, fcm: false } })
      case '/library': return json(route, { items: [film], total: 1 })
      case '/genres': return json(route, { genres: ['Drame'] })
      case '/continue': case '/history': return json(route, { items: [], total: 0 })
      case '/notifications': return json(route, { items: [], last_id: 0 })
      case '/requests/mine': return json(route, { items: [] })
      case `/titles/${film.id}`: return json(route, { ...film, ...pistes })
      case `/titles/${film.id}/playback`:
        ouvertures.push(route.request().postDataJSON())
        return json(route, { stream_id: 's1', mode: 'direct', url: '/stream/s1/direct', duration: film.duration, audio: pistes.audio, subtitles: pistes.subtitles.map((t) => ({ ...t, url: `/stream/s1/sub/${t.n}` })) })
      default: return json(route, { error: 'not_found', message: 'introuvable' }, 404)
    }
  })
  return { erreursEstompe, appelsEstompe, ouvertures }
}

test('Entracte seul : la section Entracte, sans messagerie', async ({ page }) => {
  const s = await simuler(page, { estompe: false, entracte: true })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Entracte', level: 1 })).toBeVisible()
  await expect(page.locator('.film-card strong', { hasText: 'Le Film du soir' })).toBeVisible()
  await expect(page).toHaveURL(/#\/films/)
  await expect(page.getByRole('navigation', { name: 'Navigation principale' })).toHaveCount(0)
  await expect(page.getByRole('link', { name: 'Mes demandes' })).toBeVisible()
  await expect(page.getByText('Messages privés')).toHaveCount(0)
  expect(s.erreursEstompe).toEqual([])
  expect(s.appelsEstompe.filter((c) => c !== '/me')).toEqual([])
})

test('Estompe seul : la messagerie, aucune entrée Entracte', async ({ page }) => {
  await simuler(page, { estompe: true, entracte: false })
  await page.goto('/')
  await expect(page.getByRole('navigation', { name: 'Navigation principale' })).toBeVisible()
  await expect(page.getByRole('link', { name: /Entracte/ })).toHaveCount(0)
  await page.goto('/#/films')
  await expect(page.getByRole('heading', { name: 'Entracte', level: 1 })).toHaveCount(0)
})

test('Estompe et Entracte : la section Entracte remplace l’onglet films', async ({ page }) => {
  await simuler(page, { estompe: true, entracte: true })
  await page.goto('/')
  const entree = page.getByRole('link', { name: 'Entracte, films et séries' })
  await expect(entree).toBeVisible()
  await entree.click()
  await expect(page.getByRole('heading', { name: 'Entracte', level: 1 })).toBeVisible()
  await expect(page.locator('.film-card strong', { hasText: 'Le Film du soir' })).toBeVisible()
  await expect(page.getByRole('searchbox', { name: 'Rechercher un film ou une série' })).toBeVisible()
})

test('Lecteur : langue, sous-titres et volume retenus, commandes au clavier', async ({ page }) => {
  const s = await simuler(page, { estompe: false, entracte: true })
  await page.goto(`/#/films/watch/${film.id}`)
  await page.getByRole('button', { name: 'Audio et sous-titres' }).click()
  await page.getByLabel('Audio', { exact: true }).selectOption('vff')
  await page.getByLabel('Sous-titres', { exact: true }).selectOption('forced')
  await expect(page.getByRole('button', { name: 'VFF (AC3, 5.1)' })).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByRole('button', { name: 'Français forcés' })).toHaveAttribute('aria-pressed', 'true')
  await page.getByRole('button', { name: 'Audio et sous-titres' }).click()

  const volume = page.getByRole('slider', { name: 'Volume' })
  await page.locator('.film-player-video').click({ position: { x: 5, y: 200 } })
  await page.keyboard.press('ArrowDown')
  await page.keyboard.press('ArrowDown')
  await expect(volume).toHaveValue('90')
  await page.keyboard.press('m')
  await expect(page.getByRole('button', { name: 'Rétablir le son' })).toBeVisible()
  await page.keyboard.press('m')
  await expect(volume).toHaveValue('90')

  s.ouvertures.length = 0
  await page.reload()
  await expect(page.getByRole('slider', { name: 'Volume' })).toHaveValue('90')
  await page.getByRole('button', { name: 'Audio et sous-titres' }).click()
  await expect(page.getByRole('button', { name: 'VFF (AC3, 5.1)' })).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByRole('button', { name: 'Français forcés' })).toHaveAttribute('aria-pressed', 'true')
  expect([...new Set(s.ouvertures.map((o) => o.audio))]).toEqual([2])
})
