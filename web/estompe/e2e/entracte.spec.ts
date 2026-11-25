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

type Profil = { estompe: boolean; entracte: boolean; admin?: boolean; historique?: unknown[] }

const demandeAcceptee = { id: '22222222-2222-2222-2222-222222222222', kind: 'movie', tmdb_id: 501, seasons: null, title: 'Le Règne animal', year: 2023, poster_url: null, state: 'acceptee', reason: null, created_at: '2026-11-18T21:00:00.000Z', updated_at: '2026-11-18T21:00:00.000Z', sent: true, send_error: null, requesters: [{ name: 'camille', joined_at: '2026-11-18T21:00:00.000Z' }] }
const demandeEnCours = { ...demandeAcceptee, id: '33333333-3333-3333-3333-333333333333', tmdb_id: 502, title: 'Dune : Deuxième partie', year: 2024 }
const titresArr = [
  { service: 'radarr', id: 41, tmdb_id: 501, title: 'Le Règne animal', year: 2023, monitored: true, profile: 7, folder: '/films', status: 'released', size: 0, has_file: false, available: true, file: null },
  { service: 'radarr', id: 42, tmdb_id: 502, title: 'Dune : Deuxième partie', year: 2024, monitored: true, profile: 7, folder: '/films', status: 'released', size: 0, has_file: false, available: true, file: null },
  { service: 'sonarr', id: 7, tmdb_id: 601, title: 'Andor', year: 2022, monitored: true, profile: 3, folder: '/series', status: 'ended', size: 152471334912, episodes: 24, episode_files: 20, monitor_new: true, seasons: [{ number: 0, monitored: false, episodes: 2, episode_files: 0, size: 0 }, { number: 1, monitored: true, episodes: 12, episode_files: 8, size: 60000000000 }, { number: 2, monitored: true, episodes: 12, episode_files: 12, size: 92000000000 }] },
]
const fileArr = [
  { service: 'radarr', id: 55, title_id: 42, tmdb_id: 502, title: 'Dune : Deuxième partie', year: 2024, episode: null, release: 'Dune.Part.Two.2024.MULTI.1080p.BluRay.x264-GROUPE', quality: 'Bluray-1080p', indexer: 'Indexeur 1', client: 'qBittorrent', protocol: 'torrent', size: 15246635827, left: 5793721614, progress: 62, state: 'downloading', messages: [], eta: null, importable: true },
  { service: 'sonarr', id: 56, title_id: 7, tmdb_id: 601, title: 'Andor', year: 2022, episode: { id: 704, season: 1, number: 4, title: 'Aldhani' }, release: 'Andor.S01E04.FRENCH.1080p.WEB.H264-GROUPE', quality: 'WEBDL-1080p', indexer: 'Indexeur 3', client: 'qBittorrent', protocol: 'torrent', size: 3543348019, left: 0, progress: 100, state: 'blocked', messages: ['Aucun fichier vidéo reconnu dans le téléchargement'], eta: null, importable: true },
]
const releasesArr = [
  { guid: 'g-1', indexer_id: 2, title: 'Le.Regne.Animal.2023.FRENCH.1080p.BluRay.x264-GROUPE', indexer: 'Indexeur 1', protocol: 'torrent', quality: 'Bluray-1080p', languages: ['French'], formats: [], score: 120, size: 12240656794, seeders: 48, leechers: 3, age: 412, approved: true, rejections: [], full_season: false },
  { guid: 'g-2', indexer_id: 2, title: 'Le.Regne.Animal.2023.FRENCH.2160p.UHD.BluRay.REMUX-GROUPE', indexer: 'Indexeur 3', protocol: 'torrent', quality: 'Remux-2160p', languages: ['French'], formats: ['HDR'], score: 150, size: 62384396698, seeders: 14, leechers: 2, age: 402, approved: false, rejections: ['La qualité Remux-2160p n’est pas dans le profil'], full_season: false },
]
const ficheSerie = { ...titresArr[2], profiles: [{ id: 3, name: 'WEB-1080p' }, { id: 4, name: 'Ultra-HD' }], episode_list: [
  { id: 701, season: 1, number: 1, title: 'Kassa', monitored: true, has_file: true, air_date: '2022-09-21T00:00:00Z', quality: 'WEBDL-1080p', size: 3328599654 },
  { id: 704, season: 1, number: 4, title: 'Aldhani', monitored: true, has_file: false, air_date: '2022-09-28T00:00:00Z', quality: null, size: null },
  { id: 710, season: 1, number: 10, title: 'Une seule issue', monitored: true, has_file: false, air_date: '2022-11-09T00:00:00Z', quality: null, size: null },
] }
const manquantsArr = [
  { service: 'radarr', title_id: 41, tmdb_id: 501, title: 'Le Règne animal', year: 2023, profile: 7, last_search: null },
  { service: 'sonarr', title_id: 7, tmdb_id: 601, title: 'Andor', year: 2022, episode: { id: 710, season: 1, number: 10, title: 'Une seule issue' }, air_date: '2022-11-09T00:00:00Z', last_search: null },
]

function administration(chemin: string, methode: string): unknown {
  if (methode !== 'GET') return {}
  switch (chemin) {
    case '/admin/requests': return { items: [demandeAcceptee, demandeEnCours] }
    case '/admin/status': return { sync: [{ source: 'radarr', succeeded_at: '2026-11-19T14:02:00.000Z', attempted_at: null, error: null }, { source: 'sonarr', succeeded_at: '2026-11-19T14:02:00.000Z', attempted_at: null, error: null }], transcoding: { requested: 'auto', active: 'cpu', message: null, running: 0 }, configured: { radarr: true, sonarr: true, tmdb: true, push: false } }
    case '/admin/arr/titles': return { items: titresArr, errors: [], services: { radarr: true, sonarr: true } }
    case '/admin/arr/queue': return { items: fileArr, errors: [] }
    case '/admin/arr/radarr/releases': return { items: releasesArr }
    case '/admin/arr/missing': return { items: manquantsArr, totals: { radarr: 1, sonarr: 4 }, errors: [] }
    case '/admin/arr/history': return { items: [{ service: 'sonarr', title_id: 7, tmdb_id: 601, title: 'Andor', year: 2022, episode: { id: 704, season: 1, number: 4, title: 'Aldhani' }, event: 'failed', release: 'Andor.S01E04.FRENCH.1080p.WEB.H264-GROUPE', quality: 'WEBDL-1080p', date: '2026-11-19T13:00:00Z', indexer: 'Indexeur 3', reason: 'Aucun fichier vidéo' }], errors: [] }
    case '/admin/arr/blocklist': return { items: [], errors: [] }
    case '/admin/arr/sonarr/titles/7': return ficheSerie
    default: return null
  }
}

const json = (route: Route, body: unknown, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body), headers: { 'access-control-allow-origin': 'http://localhost:4174', 'access-control-allow-credentials': 'true' } })

async function simuler(page: Page, p: Profil) {
  const erreursEstompe: string[] = []
  const appelsEstompe: string[] = []
  const ouvertures: { audio: number }[] = []
  const ecrituresAdmin: string[] = []
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
    if (p.admin && url.pathname.startsWith('/admin/')) {
      const methode = route.request().method()
      if (methode !== 'GET') ecrituresAdmin.push(`${methode} ${url.pathname}${url.search} ${route.request().postData() ?? ''}`.trim())
      const corps = administration(url.pathname, methode)
      return corps === null ? json(route, { error: 'not_found', message: 'introuvable' }, 404) : json(route, corps)
    }
    switch (url.pathname) {
      case '/me': return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: p.entracte, request: p.entracte, admin: !!p.admin }, push: { web: false, fcm: false } })
      case '/library': return json(route, { items: [film], total: 1 })
      case '/genres': return json(route, { genres: ['Drame'] })
      case '/continue': return json(route, { items: [], total: 0 })
      case '/history': return json(route, { items: p.historique ?? [], total: (p.historique ?? []).length })
      case '/notifications': return json(route, { items: [], last_id: 0 })
      case '/requests/mine': return json(route, { items: [] })
      case `/titles/${film.id}`: return json(route, { ...film, ...pistes })
      case `/titles/${film.id}/playback`:
        ouvertures.push(route.request().postDataJSON())
        return json(route, { stream_id: 's1', mode: 'direct', url: '/stream/s1/direct', duration: film.duration, audio: pistes.audio, subtitles: pistes.subtitles.map((t) => ({ ...t, url: `/stream/s1/sub/${t.n}` })) })
      default: return json(route, { error: 'not_found', message: 'introuvable' }, 404)
    }
  })
  return { erreursEstompe, appelsEstompe, ouvertures, ecrituresAdmin }
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

test('Historique : un jour par section, les épisodes d’une série regroupés', async ({ page }) => {
  const maintenant = Date.now()
  const episode = (n: number, serie: string, titre: string, vu: number) => ({ ...film, id: `44444444-4444-4444-4444-${serie}${String(n).padStart(8, '0')}`, kind: 'episode', title: `Épisode ${n}`, season: 1, episode: n, series_id: `55555555-5555-5555-5555-${serie}00000000`, series_title: titre, progress: { position: 3000, duration: 3000, seen: true }, seen_at: new Date(vu).toISOString() })
  const hier = maintenant - 86_400_000
  await simuler(page, { estompe: false, entracte: true, historique: [
    episode(9, 'aaaa', 'Les Veilleurs', maintenant),
    episode(4, 'bbbb', 'La Combe', maintenant),
    episode(8, 'aaaa', 'Les Veilleurs', maintenant),
    episode(7, 'aaaa', 'Les Veilleurs', maintenant),
    { ...film, progress: { position: 5400, duration: 5400, seen: true }, available: false, seen_at: new Date(hier).toISOString() },
    episode(6, 'aaaa', 'Les Veilleurs', hier),
  ] })
  await page.goto('/#/films/seen')
  await expect(page.getByRole('heading', { name: 'Historique', level: 2 })).toBeVisible()
  await expect(page.getByText('6 titres vus')).toBeVisible()
  const aujourdhui = page.getByRole('region', { name: 'Aujourd’hui' })
  await expect(aujourdhui.locator('.film-vu')).toHaveCount(2)
  await expect(aujourdhui.locator('.film-vu').first()).toContainText('Les Veilleurs')
  await expect(aujourdhui.locator('.film-vu').first()).toContainText('S1 · E7 à E9 · 3 épisodes')
  await expect(aujourdhui.locator('.film-vu').nth(1)).toContainText('S1 · E4 · Épisode 4')
  const veille = page.getByRole('region', { name: 'Hier' })
  await expect(veille.locator('.film-vu')).toHaveCount(2)
  await expect(veille.locator('.film-vu').first()).toContainText('Film · 2021 · 1 h 30')
  await expect(veille.locator('.film-vu').first()).toContainText('Indisponible')
  await expect(veille.locator('.film-vu').nth(1)).toContainText('S1 · E6 · Épisode 6')
  if (process.env.CAPTURE_HISTORIQUE) await page.screenshot({ path: process.env.CAPTURE_HISTORIQUE })
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
  const modes = () => page.evaluate(() => [...document.querySelector('video')!.textTracks].map((t) => t.mode))
  await expect.poll(modes).toEqual(['disabled', 'disabled', 'showing'])
  await page.getByRole('button', { name: 'Aucun', exact: true }).click()
  await expect.poll(modes).toEqual(['disabled', 'disabled', 'disabled'])
  await page.reload()
  await page.getByRole('button', { name: 'Audio et sous-titres' }).click()
  await expect(page.getByRole('button', { name: 'Français forcés' })).toHaveAttribute('aria-pressed', 'true')
  await expect.poll(modes).toEqual(['disabled', 'disabled', 'showing'])
})

test('Administration : suivi des demandes, recherche interactive, file, manquants et fiche d’un titre', async ({ page }) => {
  const s = await simuler(page, { estompe: false, entracte: true, admin: true })
  await page.goto('/#/films/admin')
  const enCours = page.locator('li', { hasText: 'Dune : Deuxième partie (2024)' })
  await expect(enCours.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '62')
  const sansRien = page.locator('li', { hasText: 'Le Règne animal (2023)' })
  await expect(sansRien).toContainText('Aucun téléchargement en cours.')
  await sansRien.getByRole('button', { name: 'Relancer la recherche' }).click()
  await expect.poll(() => s.ecrituresAdmin).toContain('POST /admin/arr/radarr/search {"title":41}')

  await sansRien.getByRole('button', { name: 'Choisir une release…' }).click()
  await expect(page.getByRole('heading', { name: 'Le Règne animal (2023)' })).toBeVisible()
  await expect(page.getByText('Rejetée par Radarr : La qualité Remux-2160p n’est pas dans le profil')).toBeVisible()
  await page.getByRole('button', { name: 'Télécharger Le.Regne.Animal.2023.FRENCH.1080p.BluRay.x264-GROUPE' }).click()
  await expect(page.getByText('Envoyée', { exact: true })).toBeVisible()
  expect(s.ecrituresAdmin).toContain('POST /admin/arr/radarr/releases {"guid":"g-1","indexer_id":2}')
  await page.getByRole('button', { name: 'Retour' }).click()

  await page.getByRole('button', { name: /^Téléchargements/ }).click()
  const bloque = page.locator('li', { hasText: 'Andor (2022) · S01E04' })
  await expect(bloque.getByRole('alert')).toContainText('Aucun fichier vidéo reconnu')
  await bloque.getByRole('button', { name: 'Autre version' }).click()
  await page.getByRole('button', { name: 'Chercher une autre version' }).click()
  await expect.poll(() => s.ecrituresAdmin).toContain('DELETE /admin/arr/sonarr/queue/56?blocklist=true&search=true')

  await page.getByRole('button', { name: 'Manquants' }).click()
  await expect(page.getByText('Andor (2022) · Saison 1')).toBeVisible()
  await page.getByRole('button', { name: 'Rechercher S01E10' }).click()
  await expect.poll(() => s.ecrituresAdmin).toContain('POST /admin/arr/sonarr/search {"episodes":[710]}')

  await page.getByRole('button', { name: 'Gérer Andor (2022)' }).click()
  await expect(page.getByRole('heading', { name: 'Andor (2022)' })).toBeVisible()
  await page.getByRole('button', { name: 'Épisodes' }).first().click()
  await expect(page.getByText('Aldhani')).toBeVisible()
  await page.getByRole('checkbox', { name: 'Surveiller S01E10' }).uncheck()
  await expect.poll(() => s.ecrituresAdmin).toContain('PUT /admin/arr/sonarr/episodes/monitor {"episodes":[710],"monitored":false}')
  await page.getByRole('button', { name: 'Retour' }).click()

  await page.getByRole('button', { name: 'Catalogue' }).click()
  await expect(page.locator('li', { hasText: 'Andor (2022)' })).toContainText('20 épisodes sur 24 En cours')
})
