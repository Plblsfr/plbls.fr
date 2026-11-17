import { expect, test, type Page, type Route } from '@playwright/test'

const ORIGINE = 'http://localhost:4176'
const ESTOMPE = 'http://estompe.test'
const REGIE = 'http://regie.test'

const cors = { 'access-control-allow-origin': ORIGINE, 'access-control-allow-credentials': 'true' }
const json = (route: Route, body: unknown, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body), headers: cors })
const preflight = (route: Route) => route.fulfill({ status: 204, headers: { ...cors, 'access-control-allow-headers': '*', 'access-control-allow-methods': '*' } })

const service = (key: string, name: string, plus: Record<string, unknown> = {}) => ({ key, name, configured: true, reachable: true, version: '1.2.3', error: null, alerts: [], ...plus })

function etatInitial() {
  return {
    services: [
      service('radarr', 'Radarr', { alerts: [{ level: 'warning', source: 'IndexerStatusCheck', message: 'Indexeurs indisponibles : Alpha' }] }),
      service('sonarr', 'Sonarr', { reachable: false, version: null, error: 'injoignable (connexion refusée)' }),
      service('lidarr', 'Lidarr', { configured: false, reachable: false, version: null }),
      service('prowlarr', 'Prowlarr'),
      service('qbittorrent', 'qBittorrent', { version: '5.1.0', transfer: { download_speed: 3 * 1024 * 1024, upload_speed: 512 * 1024, connection: 'connected' } }),
    ],
    indexers: [
      { id: 2, name: 'Bêta', enabled: true, protocol: 'torrent', privacy: 'public', failing: false, disabled_until: null, last_failure: null },
      { id: 1, name: 'Alpha', enabled: true, protocol: 'torrent', privacy: 'private', failing: true, disabled_until: '2026-11-17T12:00:00Z', last_failure: '2026-11-17T10:00:00Z' },
    ],
    disks: [{ service: 'radarr', path: '/srv/medias/films', free: 250 * 1024 ** 3, accessible: true }, { service: 'sonarr', path: '/srv/medias/series', free: null, accessible: false }],
    queue: [{ service: 'radarr', id: 7, title: 'Le.Film.du.soir.2021.1080p', level: 'warning', state: 'importBlocked', messages: ['Qualité inférieure au fichier présent'] }],
  }
}

const H1 = 'a1'.repeat(20)
const H2 = 'b2'.repeat(20)
const torrent = (plus: Record<string, unknown>) => ({ state: 'downloading', progress: 0, size: 0, download_speed: 0, upload_speed: 0, eta: 8640000, ratio: 0, category: '', tags: [] as string[], added_at: 0, seeds: 0, peers: 0, download_limit: 0, upload_limit: 0, title: null, ...plus })

function telechargements() {
  return {
    version: 1,
    error: null,
    categories: ['radarr', 'sonarr'],
    tags: ['vf'],
    transfer: { download_speed: 3 * 1024 * 1024, upload_speed: 512 * 1024, download_limit: 0, upload_limit: 0, alternative: false, free_space: 250 * 1024 ** 3, connection: 'connected' },
    torrents: [
      torrent({ hash: H1, name: 'Le.Film.du.soir.2021.1080p', progress: 0.42, size: 4 * 1024 ** 3, download_speed: 3 * 1024 * 1024, eta: 780, category: 'radarr', tags: ['vf'], seeds: 12, title: { service: 'radarr', title: 'Le Film du soir (2021)', detail: null } }),
      torrent({ hash: H2, name: 'Ancien.Film.1999', state: 'stalledUP', progress: 1, size: 700 * 1024 ** 2, ratio: 2.5 }),
    ],
  }
}

async function simuler(page: Page, admin: boolean) {
  const etat = etatInitial()
  const appels: string[] = []
  const dl = telechargements()
  const fichiers = [
    { index: 0, name: 'Ancien.Film.1999/film.mkv', size: 690 * 1024 ** 2, progress: 1, priority: 1 },
    { index: 1, name: 'Ancien.Film.1999/sample.mkv', size: 10 * 1024 ** 2, progress: 0, priority: 0 },
  ]
  const attente: Route[] = []
  const publier = () => { dl.version++; for (const r of attente.splice(0)) void json(r, dl) }
  await page.route(`${ESTOMPE}/**`, (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() === 'OPTIONS') return preflight(route)
    switch (url.pathname) {
      case '/me': return json(route, { activated: true, allowed: true, admin: false, rights: ['access'], id: 'me', pseudo: 'moi', sub: 'kc-moi', blocks: [] })
      case '/unread': return json(route, { channels: [], dms: [], counts: {} })
      case '/me/notify-settings': return json(route, { levels: {}, types: { dm: true, mention: true, channel: true, voice: true }, preview: true, sound: true })
      case '/avatars': case '/statuses': return json(route, {})
      case '/ws-ticket': return json(route, { ticket: 't' })
      default: return json(route, [])
    }
  })
  await page.route(`${REGIE}/**`, (route) => {
    const url = new URL(route.request().url())
    const methode = route.request().method()
    if (methode === 'OPTIONS') return preflight(route)
    if (!['/me', '/admin/status', '/admin/downloads'].includes(url.pathname) && !url.pathname.startsWith('/admin/deposits')) appels.push(`${methode} ${url.pathname}${url.search}`)
    if (url.pathname === '/me') return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { admin } })
    if (!admin) return json(route, { error: 'forbidden', message: 'permission regie:admin requise' }, 403)
    if (url.pathname === '/admin/status') return json(route, etat)
    if (url.pathname.startsWith('/admin/deposits') || url.pathname.startsWith('/admin/library')) return depots(route, url, methode)
    if (url.pathname === '/admin/downloads') {
      if (url.searchParams.get('since') === String(dl.version)) { attente.push(route); return }
      return json(route, dl)
    }
    if (methode === 'POST' && url.pathname.startsWith('/admin/downloads/')) {
      const corps = route.request().postDataJSON() as { hashes?: string[]; delete_files?: boolean; ids?: number[]; priority?: number }
      appels[appels.length - 1] += ` ${JSON.stringify(corps)}`
      const cible = dl.torrents.find((t) => corps.hashes?.includes(t.hash))
      if (url.pathname.endsWith('/pause') && cible) cible.state = 'stoppedDL'
      if (url.pathname.endsWith('/resume') && cible) cible.state = 'downloading'
      if (url.pathname.endsWith('/delete')) dl.torrents = dl.torrents.filter((t) => !corps.hashes?.includes(t.hash))
      if (url.pathname.endsWith('/files')) fichiers[1].priority = corps.priority ?? 1
      publier()
      return route.fulfill({ status: 204, headers: cors })
    }
    if (url.pathname === `/admin/downloads/${H2}`) return json(route, { hash: H2, files: fichiers, title: { service: 'radarr', title: 'Ancien Film (1999)', detail: null } })
    if (methode === 'POST' && url.pathname === '/admin/indexers/1/test') return json(route, { ok: false, message: 'Connexion refusée par le site' })
    if (methode === 'POST' && url.pathname === '/admin/indexers/2/test') return json(route, { ok: true, message: null })
    if (methode === 'DELETE' && url.pathname === '/admin/queue/radarr/7') { etat.queue = []; return route.fulfill({ status: 204, headers: cors }) }
    return json(route, { error: 'not_found', message: 'introuvable' }, 404)
  })
  const DEPOT = 'c3'.repeat(20)
  const depot = { id: DEPOT, name: 'Film.2024', state: 'upload', created_at: 1_794_000_000, tracker: 'tracker.exemple.fr', private: true, size: 8, received: 0, imported: [] as string[], error: null as string | null,
    files: [{ index: 0, path: 'Film.2024/film.mkv', size: 5, received: 0, valid: null as number | null }, { index: 1, path: 'Film.2024/Subs/fr.srt', size: 3, received: 0, valid: null as number | null }] }
  const etatDepots = { cree: false, verifications: 0, attente: [] as Route[], recus: [] as string[] }
  const finir = (state: string, valides: (number | null)[]) => {
    depot.state = state
    depot.files.forEach((f, i) => (f.valid = valides[i]))
    for (const r of etatDepots.attente.splice(0)) void json(r, depot)
  }
  function depots(route: Route, url: URL, methode: string) {
    const chemin = url.pathname
    if (chemin === '/admin/deposits' && methode === 'GET') return json(route, { items: etatDepots.cree ? [depot] : [], used: etatDepots.cree ? 8 : 0, limit: 200 * 1024 ** 3 })
    if (chemin === '/admin/deposits' && methode === 'POST') { etatDepots.cree = true; return json(route, depot, 201) }
    if (chemin === `/admin/deposits/${DEPOT}` && methode === 'GET') {
      if (url.searchParams.get('wait') === depot.state) { etatDepots.attente.push(route); return }
      return json(route, depot)
    }
    if (chemin === `/admin/deposits/${DEPOT}` && methode === 'DELETE') { etatDepots.cree = false; return route.fulfill({ status: 204, headers: cors }) }
    const fichier = chemin.match(/\/files\/(\d)$/)
    if (fichier && methode === 'PUT') {
      const f = depot.files[Number(fichier[1])]
      const corps = route.request().postDataBuffer() ?? Buffer.alloc(0)
      etatDepots.recus.push(`${f.path}@${url.searchParams.get('offset')}${url.searchParams.get('reset') ? '!' : ''}=${corps.toString()}`)
      f.received = Number(url.searchParams.get('offset')) + corps.length
      if (url.searchParams.get('reset')) f.valid = null
      depot.received = depot.files.reduce((n, x) => n + x.received, 0)
      return json(route, { received: f.received })
    }
    if (chemin.endsWith('/seed')) { etatDepots.verifications++; depot.state = 'checking'; return json(route, depot) }
    if (chemin.endsWith('/import') && methode === 'GET') {
      return json(route, { service: url.searchParams.get('service'), items: url.searchParams.get('service') === 'radarr'
        ? [{ path: 'Film.2024/film.mkv', size: 5, title: { service: 'radarr', title: 'Le Film (2024)', detail: null }, recognized: true, reasons: [] }, { path: 'Film.2024/Subs/fr.srt', size: 3, title: null, recognized: false, reasons: ['Unknown Movie'] }]
        : [] })
    }
    if (chemin.endsWith('/import') && methode === 'POST') {
      const corps = route.request().postDataJSON() as { service: string; movie_id?: number }
      depot.imported = [corps.service]
      return json(route, { service: corps.service, files: corps.movie_id ? 2 : 1 })
    }
    if (chemin === '/admin/library/radarr') return json(route, { items: [{ id: 5, title: 'Le Film du dimanche', detail: null, year: 1987 }] })
    if (chemin === '/admin/library/radarr/5/sheet') {
      return json(route, { service: 'radarr', id: 5, title: 'Le Film du dimanche', original_title: null, artist: null, year: 1987, runtime: 95, genres: ['Comédie'], ids: { tmdb: 42, imdb: null, tvdb: null, musicbrainz: null },
        files: [{ name: 'Le.Film.du.dimanche.1987.1080p.mkv', group: null, size: 4 * 1024 ** 3, quality: 'Bluray-1080p', release_group: 'GRP', scene_name: null, edition: null, languages: ['French'],
          video: { codec: 'x264', resolution: '1920x1080', bit_depth: 8, dynamic_range: null, fps: 24, bitrate: 6_000_000, scan: null }, audio: { codec: 'AC3', channels: 2, languages: 'fre', bitrate: 192_000, streams: 1, sample_rate: null, bits: null }, subtitles: 'fre', duration: '1:35:00' }] })
    }
    return json(route, { error: 'not_found', message: 'introuvable' }, 404)
  }
  return { appels, etatDepots, finir }
}

test('Sans regie:admin : aucune entrée Régie, et #/regie ramène à l’accueil', async ({ page }) => {
  await simuler(page, false)
  await page.goto('/')
  await expect(page.getByRole('navigation', { name: 'Navigation principale' })).toBeVisible()
  await expect(page.getByRole('link', { name: /Régie/ })).toHaveCount(0)
  await page.goto('/#/regie')
  await expect(page.getByRole('heading', { name: 'Régie', level: 1 })).toHaveCount(0)
  await expect(page).not.toHaveURL(/#\/regie/)
})

test('Avec regie:admin : santé des services, indexeurs, disques et téléchargements bloqués', async ({ page }) => {
  const s = await simuler(page, true)
  await page.goto('/')
  await page.getByRole('link', { name: 'Régie, administration des médias' }).click()
  await expect(page.getByRole('heading', { name: 'Régie', level: 1 })).toBeVisible()
  await expect(page.getByText('5 à traiter')).toBeVisible()

  const services = page.locator('section', { has: page.getByRole('heading', { name: 'Services' }) }).getByRole('listitem')
  await expect(services).toHaveCount(5)
  await expect(services.nth(0)).toContainText('Radarr')
  await expect(services.nth(0)).toContainText('Indexeurs indisponibles : Alpha')
  await expect(services.nth(0)).toContainText('Avertissement')
  await expect(services.nth(1)).toContainText('Injoignable')
  await expect(services.nth(2)).toContainText('Non configuré')
  await expect(services.nth(3)).toContainText('OK')
  await expect(services.nth(4)).toContainText('Réception 3 Mo/s · envoi 512 Ko/s')

  const indexeurs = page.locator('section', { has: page.getByRole('heading', { name: 'Indexeurs' }) }).getByRole('listitem')
  await expect(indexeurs.nth(0)).toContainText('Alpha')
  await expect(indexeurs.nth(0)).toContainText('En panne')
  await expect(indexeurs.nth(1)).toContainText('Bêta')
  await indexeurs.nth(0).getByRole('button', { name: 'Tester' }).click()
  await expect(page.getByText('« Alpha » : Connexion refusée par le site')).toBeVisible()
  await indexeurs.nth(1).getByRole('button', { name: 'Tester' }).click()
  await expect(page.getByText('« Bêta » répond.')).toBeVisible()

  const disques = page.locator('section', { has: page.getByRole('heading', { name: 'Disques' }) }).getByRole('listitem')
  await expect(disques.nth(0)).toContainText('250 Go libres')
  await expect(disques.nth(1)).toContainText('Inaccessible')

  const bloques = page.locator('section', { has: page.getByRole('heading', { name: 'Téléchargements bloqués' }) })
  await expect(bloques.getByRole('listitem')).toContainText('Qualité inférieure au fichier présent')
  await bloques.getByRole('button', { name: 'Autre version' }).click()
  await page.getByRole('alertdialog').getByRole('button', { name: 'Chercher une autre version' }).click()
  await expect(bloques.getByText('Aucun téléchargement bloqué')).toBeVisible()
  await expect(page.getByText('4 à traiter')).toBeVisible()
  expect(s.appels).toEqual(['POST /admin/indexers/1/test', 'POST /admin/indexers/2/test', 'DELETE /admin/queue/radarr/7?retry=true'])
})

test('Téléchargements : suivi en direct, pause, priorité d’un fichier, suppression', async ({ page }) => {
  const s = await simuler(page, true)
  await page.goto('/#/regie/downloads')
  await expect(page.getByRole('heading', { name: 'Téléchargements' })).toBeVisible()
  const lignes = page.locator('.rg-torrent')
  await expect(lignes).toHaveCount(2)
  await expect(lignes.nth(0)).toContainText('Le Film du soir (2021)')
  await expect(lignes.nth(0)).toContainText('Radarr · Le.Film.du.soir.2021.1080p')
  await expect(lignes.nth(0)).toContainText('42 % de 4 Go · 3 Mo/s · reste 13 min')
  await expect(lignes.nth(1)).toContainText('100 % de 700 Mo · ratio 2,5')
  await expect(page.getByText('250 Go libres')).toBeVisible()

  await page.getByRole('button', { name: /En partage/ }).click()
  await expect(lignes).toHaveCount(1)
  await page.getByRole('button', { name: /Tous/ }).click()

  await page.getByRole('button', { name: 'Suspendre Le Film du soir (2021)' }).click()
  await expect(page.getByRole('button', { name: 'Reprendre Le Film du soir (2021)' })).toBeVisible()
  await expect(lignes.nth(0)).toContainText('Suspendu')

  await page.getByRole('button', { name: 'Détails de Ancien.Film.1999' }).click()
  const fiche = page.getByRole('dialog')
  await expect(fiche).toContainText('Ancien Film (1999)')
  await expect(fiche).toContainText('Ancien.Film.1999/sample.mkv')
  await fiche.getByLabel('Priorité de Ancien.Film.1999/sample.mkv').selectOption('7')
  await expect(fiche.getByLabel('Priorité de Ancien.Film.1999/sample.mkv')).toHaveValue('7')
  await page.keyboard.press('Escape')

  await page.getByRole('button', { name: 'Actions sur Ancien.Film.1999' }).click()
  await page.getByRole('menuitem', { name: 'Supprimer avec les fichiers' }).click()
  await page.getByRole('alertdialog').getByRole('button', { name: 'Supprimer avec les fichiers' }).click()
  await expect(lignes).toHaveCount(1)
  expect(s.appels).toEqual([
    `POST /admin/downloads/pause {"hashes":["${H1}"]}`,
    `GET /admin/downloads/${H2}`,
    `POST /admin/downloads/${H2}/files {"ids":[1],"priority":7}`,
    `GET /admin/downloads/${H2}`,
    `POST /admin/downloads/delete {"hashes":["${H2}"],"delete_files":true}`,
  ])
})

test('Dépôts : .torrent, fichiers, vérification, remplacement d’un fichier non conforme, import', async ({ page }) => {
  const s = await simuler(page, true)
  await page.goto('/#/regie/deposits')
  await expect(page.getByText('Aucun dépôt.')).toBeVisible()
  await page.getByLabel('Fichier .torrent').setInputFiles({ name: 'film.torrent', mimeType: 'application/x-bittorrent', buffer: Buffer.from('d4:infoe') })
  const carte = page.locator('.rg-depot')
  await expect(carte).toContainText('Film.2024')
  await expect(carte).toContainText('tracker.exemple.fr')
  await expect(carte).toContainText('Fichiers attendus')

  await page.getByLabel('Fichiers du dépôt').setInputFiles([
    { name: 'sous-titres.srt', mimeType: 'text/plain', buffer: Buffer.from('srt') },
    { name: 'film.mkv', mimeType: 'video/x-matroska', buffer: Buffer.from('film.') },
    { name: 'intrus.txt', mimeType: 'text/plain', buffer: Buffer.from('pas attendu') },
  ])
  await expect(carte).toContainText('Prêt à vérifier')
  expect(s.etatDepots.recus).toEqual(['Film.2024/film.mkv@0=film.', 'Film.2024/Subs/fr.srt@0=srt'])

  await carte.getByRole('button', { name: 'Vérifier et partager' }).click()
  await expect(carte).toContainText('qBittorrent vérifie les fichiers')
  await expect.poll(() => s.etatDepots.attente.length).toBe(1)
  s.finir('mismatch', [1, 0.4])
  await expect(carte).toContainText('Fichiers non conformes')
  await expect(carte).toContainText('40 % conforme')
  await expect(carte.getByRole('button', { name: 'Revérifier' })).toBeDisabled()

  const choix = page.waitForEvent('filechooser')
  await carte.getByRole('button', { name: 'Remplacer fr.srt' }).click()
  await (await choix).setFiles({ name: 'bon.srt', mimeType: 'text/plain', buffer: Buffer.from('SRT') })
  await expect(carte.getByRole('button', { name: 'Revérifier' })).toBeEnabled()
  expect(s.etatDepots.recus.at(-1)).toBe('Film.2024/Subs/fr.srt@0!=SRT')
  await carte.getByRole('button', { name: 'Revérifier' }).click()
  await expect.poll(() => s.etatDepots.attente.length).toBe(1)
  s.finir('seeding', [null, null])
  await expect(carte).toContainText('En partage')
  await expect(page.getByText('« Film.2024 » est vérifié et en partage.')).toBeVisible()
  expect(s.etatDepots.verifications).toBe(2)

  await carte.getByRole('button', { name: 'Importer dans la bibliothèque' }).click()
  const fiche = page.getByRole('dialog')
  await expect(fiche).toContainText('→ Le Film (2024)')
  await expect(fiche).toContainText('Non reconnu : Unknown Movie')
  await expect(fiche.getByRole('button', { name: 'Importer 1 fichier' })).toBeEnabled()
  await fiche.getByLabel('Chercher un film de la bibliothèque').fill('dimanche')
  await fiche.getByRole('button', { name: 'Chercher' }).click()
  await fiche.getByRole('button', { name: 'Le Film du dimanche (1987)' }).click()
  await fiche.getByRole('button', { name: 'Importer 2 fichiers' }).click()
  await expect(page.getByText('2 fichiers confiés à Radarr, qui termine l’import.')).toBeVisible()
  await expect(carte).toContainText('importé dans Radarr')

  await carte.getByRole('button', { name: 'Supprimer' }).click()
  await page.getByRole('alertdialog').getByRole('button', { name: 'Supprimer' }).click()
  await expect(page.getByText('Aucun dépôt.')).toBeVisible()
})

test('Fiche technique : recherche d’un film et texte à copier', async ({ page }) => {
  await simuler(page, true)
  await page.goto('/#/regie/sheet')
  await page.getByLabel('Chercher dans la bibliothèque').fill('dimanche')
  await page.getByRole('button', { name: 'Chercher' }).click()
  await page.getByRole('button', { name: /Le Film du dimanche \(1987\)/ }).click()
  const fiche = page.getByLabel('Fiche technique')
  await expect(fiche).toContainText('Le Film du dimanche (1987)')
  await expect(fiche).toContainText('Durée : 1 h 35 · Genres : Comédie')
  await expect(fiche).toContainText('TMDB : https://www.themoviedb.org/movie/42')
  await expect(fiche).toContainText('Vidéo : x264, 1920x1080, 8 bits, 24 i/s, 6 Mb/s')
  await expect(fiche).toContainText('Audio : AC3 stéréo, fre, 192 kb/s')
  await expect(page.getByRole('button', { name: 'Copier' })).toBeVisible()
  await page.getByRole('button', { name: 'Retour aux résultats' }).click()
  await expect(page.getByRole('button', { name: /Le Film du dimanche/ })).toBeVisible()
})
