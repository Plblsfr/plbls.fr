import { expect, test, type Page, type Route } from '@playwright/test'

const ORIGINE = 'http://localhost:4175'
const ESTOMPE = 'http://estompe.test'
const ENTRACTE = 'http://entracte.test'
const SILLON = 'http://sillon.test'

type Profil = { estompe: boolean; sillon: boolean; entracte?: boolean; add?: boolean; admin?: boolean; connecte?: () => boolean }

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

type Element = { id: string; suggested: boolean; discovery: boolean; track: (typeof pistes)[number] }
type Liste = { id: string; name: string; description: null; created_at: string; updated_at: string; items: { id: string; position: number; added_at: string; track: (typeof pistes)[number] }[] }
const suggestion = { ...pistes[0], id: 'c0000000-0000-0000-0000-000000000009', title: 'Titre suggéré', album: { ...pistes[0].album, id: 'b0000000-0000-0000-0000-000000000009', title: 'Ailleurs' }, artist: { id: 'a0000000-0000-0000-0000-000000000009', name: 'Voisins' } }
const decouverte = { ...pistes[2], id: 'c0000000-0000-0000-0000-00000000000a', title: 'Titre découvert', album: { ...pistes[2].album, id: 'b0000000-0000-0000-0000-00000000000a', title: 'Lointain' }, artist: { id: 'a0000000-0000-0000-0000-00000000000a', name: 'Inconnus' } }
const catalogue = [...pistes, suggestion, decouverte]
const albumDecouvert = { ...album, id: decouverte.album.id, title: 'Lointain', added_at: '2026-10-01T10:00:00Z', track_count: 1, duration: 30, artist: { id: decouverte.artist.id, name: decouverte.artist.name } }
const GO = 1e9
const IMAGES_AJOUT: Record<string, [string, string]> = {
  a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0: ['#2f6f5e', '#e8d9a8'],
  b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1: ['#7a2e3b', '#f2c14e'],
  d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3: ['#24476b', '#9fd3c7'],
}
const IMAGE_CASSEE = 'c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2'
const svg = ([fond, motif]: [string, string]) => `<svg xmlns="http://www.w3.org/2000/svg" width="250" height="250" viewBox="0 0 250 250"><rect width="250" height="250" fill="${fond}"/><path d="M0 250L250 0M-60 190L190-60M60 310L310 60" stroke="${motif}" stroke-width="34"/><rect x="70" y="70" width="110" height="110" fill="none" stroke="${motif}" stroke-width="14"/></svg>`

function serveurSillon() {
  let n = 0
  const uuid = () => `d0000000-0000-0000-0000-${String(++n).padStart(12, '0')}`
  const etat = {
    playlists: [] as Liste[],
    likes: { tracks: [] as string[], albums: [] as string[], artists: [] as string[] },
    file: { revision: 0, source: null as null | { type: string; id: string | null; name: string }, index: 0, position: 0, shuffle: false, repeat: 'off', suggestions: 'off', seed: null, updated_at: null, items: [] as Element[] },
    ecoutes: [] as Record<string, unknown>[],
    ecartees: [] as string[],
    ajouts: [
      { id: 'f0000000-0000-0000-0000-000000000001', kind: 'album', mbid: 'e0000000-0000-0000-0000-000000000004', title: 'Introuvable', artist: { mbid: 'e0000000-0000-0000-0000-000000000002', name: 'Les Absents' }, origin: 'manual', state: 'failed', reason: 'aucune source trouvée par Lidarr', estimated_size: 400e6, albums: [], created_at: '2026-11-15T18:00:00Z', updated_at: '2026-11-16T08:00:00Z', author: { sub: 'kc-moi', name: 'moi' } },
      { id: 'f0000000-0000-0000-0000-000000000002', kind: 'album', mbid: 'e0000000-0000-0000-0000-000000000005', title: 'En Route', artist: { mbid: 'e0000000-0000-0000-0000-000000000002', name: 'Les Absents' }, origin: 'manual', state: 'downloading', reason: null, estimated_size: 350e6, albums: [], image: 'd3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3', created_at: '2026-11-16T09:30:00Z', updated_at: '2026-11-16T09:40:00Z', author: { sub: 'kc-moi', name: 'moi' } },
      { id: 'f0000000-0000-0000-0000-000000000003', kind: 'album', mbid: 'e0000000-0000-0000-0000-000000000006', title: 'Premier Pressage', artist: { mbid: 'e0000000-0000-0000-0000-000000000007', name: 'Les Sillons' }, origin: 'discovery', state: 'available', reason: null, estimated_size: 640e6, albums: [album.id], created_at: '2026-11-01T10:00:00Z', updated_at: '2026-11-01T12:00:00Z', author: { sub: 'kc-moi', name: 'moi' } },
    ] as Record<string, unknown>[],
    relances: [] as string[],
    supprimes: [] as string[],
    decouverteAjoutee: false,
  }
  const resume = (l: Liste) => ({ id: l.id, name: l.name, description: null, track_count: l.items.length, duration: l.items.reduce((s, e) => s + (e.track.duration ?? 0), 0), cover_albums: [], created_at: l.created_at, updated_at: l.updated_at })
  const fiche = (l: Liste) => ({ ...resume(l), items: l.items.map((e, i) => ({ ...e, position: i })) })
  const el = (t: (typeof pistes)[number], suggested = false, discovery = false): Element => ({ id: uuid(), suggested, discovery, track: t })
  const piste = (id: string) => catalogue.find((x) => x.id === id)!
  const contenu = (b: Record<string, unknown>) => (b.album ? pistes : ((b.tracks as string[] | undefined) ?? []).map(piste))
  const deplacer = <T>(l: T[], from: number, to: number) => { const r = [...l]; const [x] = r.splice(from, 1); r.splice(to, 0, x); return r }
  return { etat, resume, fiche, el, piste, contenu, deplacer, uuid }
}

async function simuler(page: Page, p: Profil, srv = serveurSillon()) {
  const appelsEstompe: string[] = []
  const lectures: { id: string; corps: { codecs: string[]; containers: string[]; quality?: string } }[] = []
  const { etat } = srv
  await page.route(`${ESTOMPE}/**`, (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() === 'OPTIONS') return preflight(route)
    appelsEstompe.push(url.pathname)
    if (p.connecte && !p.connecte()) return json(route, { error: 'unauthorized', message: 'connexion requise' }, 401)
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
      const i = catalogue.findIndex((x) => x.id === url.pathname.slice(8))
      return route.fulfill({ status: 200, contentType: 'audio/wav', body: wav(catalogue[i].duration ?? 3), headers: { 'access-control-allow-origin': '*', 'accept-ranges': 'none' } })
    }
    if (p.connecte && !p.connecte()) return json(route, { error: 'unauthorized', message: 'connexion requise' }, 401)
    const m = req.method()
    const corps = (m === 'GET' || m === 'DELETE' ? {} : (req.postDataJSON() ?? {})) as Record<string, unknown>
    const file = () => json(route, etat.file)
    const ecrireFile = (f: () => void) => {
      if (corps.revision !== etat.file.revision) return json(route, { error: 'conflict', message: 'la file a été modifiée sur un autre appareil', queue: etat.file }, 409)
      f()
      etat.file.revision++
      return file()
    }
    const pl = /^\/playlists\/([^/]+)(\/tracks(?:\/([^/]+))?|\/move)?$/.exec(url.pathname)
    if (pl) {
      const l = etat.playlists.find((x) => x.id === pl[1])
      if (!l) return json(route, { error: 'not_found', message: 'introuvable' }, 404)
      if (!pl[2] && m === 'GET') return json(route, srv.fiche(l))
      if (!pl[2] && m === 'PATCH') { l.name = String(corps.name); return json(route, srv.fiche(l)) }
      if (!pl[2] && m === 'DELETE') { etat.playlists = etat.playlists.filter((x) => x !== l); return route.fulfill({ status: 204, headers: cors }) }
      if (pl[2] === '/move') { l.items = srv.deplacer(l.items, corps.from as number, corps.to as number); return json(route, srv.fiche(l)) }
      if (pl[3] && m === 'DELETE') { l.items = l.items.filter((e) => e.id !== pl[3]); return json(route, srv.fiche(l)) }
      const ajout = srv.contenu(corps).map((track) => ({ id: srv.uuid(), position: 0, added_at: '2026-11-16T10:00:00Z', track }))
      l.items.push(...ajout)
      return json(route, { ...srv.fiche(l), added: ajout.length, duplicates: [] })
    }
    const like = /^\/likes\/(tracks|albums|artists)\/([^/]+)$/.exec(url.pathname)
    if (like) {
      const k = like[1] as 'tracks'
      etat.likes[k] = m === 'PUT' ? [like[2], ...etat.likes[k].filter((x) => x !== like[2])] : etat.likes[k].filter((x) => x !== like[2])
      return route.fulfill({ status: 204, headers: cors })
    }
    const proches = /^\/tracks\/([^/]+)\/similar$/.exec(url.pathname)
    if (proches) return json(route, { items: [pistes[3], pistes[2]].slice(0, Number(url.searchParams.get('limit') ?? 20)) })
    const albumAdmin = /^\/admin\/albums\/([^/]+)$/.exec(url.pathname)
    if (albumAdmin && m === 'DELETE') { etat.supprimes.push(albumAdmin[1]); return route.fulfill({ status: 204, headers: cors }) }
    const imageAjout = /^\/additions\/images\/([^/]+)$/.exec(url.pathname)
    if (imageAjout) {
      const couleurs = IMAGES_AJOUT[imageAjout[1]]
      if (!couleurs) return json(route, { error: 'not_found', message: 'introuvable' }, 404)
      return route.fulfill({ status: 200, contentType: 'image/svg+xml', body: svg(couleurs), headers: cors })
    }
    const relance = /^\/admin\/additions\/([^/]+)\/retry$/.exec(url.pathname)
    if (relance) { etat.relances.push(relance[1]); const a = etat.ajouts.find((x) => x.id === relance[1])!; a.state = 'sent'; a.reason = null; return json(route, a, 201) }
    const playback = /^\/tracks\/([^/]+)\/playback$/.exec(url.pathname)
    if (playback && req.method() === 'POST') {
      lectures.push({ id: playback[1], corps: req.postDataJSON() })
      const d = catalogue.find((x) => x.id === playback[1])?.duration ?? 3
      return json(route, {
        track: playback[1], url: `/stream/${playback[1]}`, expires_at: '2026-11-17T00:00:00Z', mode: 'direct', quality: 'originale',
        format: { codec: 'pcm', container: 'wav', mime: 'audio/wav', bitrate: 128000 }, duration: d, samples: Math.round(d * 8000), sample_rate: 8000, output_sample_rate: 8000, encoder_delay: 0,
        gain: { reference: -18, loudness: -20, track: 2, track_peak: -6, album: 2, album_peak: -6 },
      })
    }
    switch (url.pathname) {
      case '/me': return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: p.sillon, add: !!p.add, admin: !!p.admin }, sections: p.sillon ? ['library'] : [] })
      case '/preferences': return json(route, { quality: 'originale', qualities: ['originale', 'haute', 'normale', 'basse'], discovery: true })
      case '/playlists':
        if (m === 'POST') { const l: Liste = { id: srv.uuid(), name: String(corps.name), description: null, created_at: '2026-11-16T10:00:00Z', updated_at: '2026-11-16T10:00:00Z', items: [] }; etat.playlists.unshift(l); return json(route, srv.fiche(l), 201) }
        return json(route, { items: etat.playlists.map(srv.resume) })
      case '/likes': return json(route, etat.likes)
      case '/likes/tracks': return json(route, { name: 'Titres likés', track_count: etat.likes.tracks.length, duration: 0, cover_albums: [], items: etat.likes.tracks.map((id) => ({ liked_at: '2026-11-16T10:00:00Z', track: srv.piste(id) })) })
      case '/queue':
        if (m === 'GET') return file()
        if (m === 'PUT') return ecrireFile(() => {
          const source = corps.source as { type: string; id?: string } | undefined
          const items = ((corps.tracks as string[]) ?? []).map((id) => srv.el(srv.piste(id)))
          const melange = (corps.shuffle as boolean | undefined) ?? etat.file.shuffle
          const depart = items.splice(corps.index as number, 1)
          const reste = melange ? items.reverse() : items
          const avecSuggestion = melange && etat.file.suggestions !== 'off' ? [...reste.slice(0, 1), srv.el(suggestion, true), ...reste.slice(1)] : reste
          Object.assign(etat.file, { source: source ? { type: source.type, id: source.id ?? null, name: 'Source' } : null, shuffle: melange, items: melange ? [...depart, ...avecSuggestion] : [...items.slice(0, corps.index as number), ...depart, ...items.slice(corps.index as number)], index: melange ? 0 : corps.index, position: 0 })
        })
        return ecrireFile(() => {
          if (corps.index !== undefined) { etat.file.index = corps.index as number; etat.file.position = 0 }
          if (corps.position !== undefined) etat.file.position = corps.position as number
          if (corps.repeat) etat.file.repeat = corps.repeat as string
          if (corps.suggestions) etat.file.suggestions = corps.suggestions as string
          if (corps.shuffle !== undefined) etat.file.shuffle = corps.shuffle as boolean
          if (corps.index !== undefined && etat.file.shuffle && etat.file.suggestions !== 'off' && !etat.decouverteAjoutee) {
            etat.decouverteAjoutee = true
            etat.file.items.splice(etat.file.index + 1, 0, srv.el(decouverte, true, true))
          }
        })
      case '/queue/tracks': return ecrireFile(() => { const k = corps.mode === 'next' ? etat.file.index + 1 : etat.file.items.length; etat.file.items.splice(k, 0, ...srv.contenu(corps).map((t) => srv.el(t))) })
      case '/queue/remove': return ecrireFile(() => { etat.file.items = etat.file.items.filter((e) => !(corps.items as string[]).includes(e.id)) })
      case '/queue/move': return ecrireFile(() => { etat.file.items = srv.deplacer(etat.file.items, corps.from as number, corps.to as number) })
      case '/history':
        if (m === 'POST') { etat.ecoutes.push(corps); return json(route, { id: corps.id, ended: 'completed', counted: true, suggested: false }, 201) }
        return json(route, { items: [], next_cursor: null })
      case '/history/top/tracks': case '/history/top/artists': return json(route, { items: [] })
      case '/recommendations/dismiss': etat.ecartees.push(String(corps.track)); return route.fulfill({ status: 204, headers: cors })
      case '/additions/search': return json(route, {
        artists: [
          { mbid: 'e0000000-0000-0000-0000-000000000002', name: 'Les Absents', disambiguation: 'groupe de test', type: 'Group', overview: null, image: 'a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0', library: null, in_library: false, in_progress: false, excluded: false },
          { mbid: 'e0000000-0000-0000-0000-000000000008', name: 'Sans Visage', disambiguation: null, type: 'Person', overview: null, image: null, library: null, in_library: false, in_progress: false, excluded: false },
        ],
        albums: [
          { mbid: 'e0000000-0000-0000-0000-000000000001', title: 'Trop Lourd', type: 'Album', secondary_types: [], year: 2021, duration: 3600, artist: { mbid: 'e0000000-0000-0000-0000-000000000002', name: 'Les Absents' }, image: 'b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1', library: null, in_library: false, in_progress: false, excluded: false, estimated_size: 20 * GO },
          { mbid: 'e0000000-0000-0000-0000-000000000009', title: 'Sans Pochette', type: 'EP', secondary_types: [], year: 2022, duration: 1200, artist: { mbid: 'e0000000-0000-0000-0000-000000000002', name: 'Les Absents' }, image: null, library: null, in_library: false, in_progress: false, excluded: false, estimated_size: 150e6 },
          { mbid: 'e0000000-0000-0000-0000-00000000000c', title: 'Pochette Perdue', type: 'Album', secondary_types: [], year: 2020, duration: 2000, artist: { mbid: 'e0000000-0000-0000-0000-000000000002', name: 'Les Absents' }, image: IMAGE_CASSEE, library: null, in_library: false, in_progress: false, excluded: false, estimated_size: 250e6 },
          { mbid: 'e0000000-0000-0000-0000-000000000003', title: 'Déjà Là', type: 'Album', secondary_types: [], year: 2019, duration: 2400, artist: { mbid: 'e0000000-0000-0000-0000-000000000002', name: 'Les Absents' }, image: null, library: { id: album.id, image: false }, in_library: true, in_progress: false, excluded: false, estimated_size: 300e6 },
        ],
      })
      case '/additions':
        if (corps.mbid === 'e0000000-0000-0000-0000-000000000001') return json(route, { error: 'disk_full', message: 'le disque de la musique serait plein', used: 480 * GO, pending: 10 * GO, limit: 500 * GO, free: 10 * GO, estimated: 20 * GO }, 507)
        return json(route, { id: srv.uuid(), state: 'sent' }, 201)
      case '/additions/mine': case '/admin/additions': return json(route, { items: etat.ajouts })
      case '/admin/disk': return json(route, { used: 480 * GO, pending: 10 * GO, limit: 500 * GO, free: 10 * GO, daily_limit: 5, configured: true })
      case '/admin/albums': return json(route, { items: [{ ...album, size: 640e6, added_by: 'moi' }] })
      case '/admin/status': return json(route, { sync: [{ source: 'lidarr', succeeded_at: '2026-11-16T09:00:00Z', attempted_at: '2026-11-16T09:00:00Z', error: null }], running: false, configured: { lidarr: true, webhook: true }, library: { artists: 1, albums: 1, tracks: 4, absent_tracks: 0, files: 4, rejected: 1, size: 640e6 } })
      case '/admin/rejected': return json(route, { items: [{ name: 'pochette.exe', reason: 'extension_interdite', seen_at: '2026-11-15T09:00:00Z' }] })
      case '/admin/analysis': return json(route, { version: 1, parallel: 2, paused: false, tasks: { pending: 0, running: 0, done: 4, failed: 1 }, tracks: 4, analysed: 3, loaded: 3, errors: [{ track: pistes[3].id, title: 'Morceau 4', error: 'décodage impossible', attempts: 3, at: '2026-11-16T08:00:00Z' }] })
      case '/admin/discovery': return json(route, {
        enabled: true,
        budget: { used: 12 * GO, limit: 50 * GO, daily_limit: 1, trial_days: 30 },
        items: [{ id: 'f0000000-0000-0000-0000-00000000000a', kind: 'album', mbid: 'e0000000-0000-0000-0000-00000000000a', title: 'Lointain', artist: { mbid: 'e0000000-0000-0000-0000-00000000000b', name: 'Inconnus' }, origin: 'discovery', state: 'available', reason: null, estimated_size: 400e6, albums: [albumDecouvert.id], created_at: '2026-10-01T09:00:00Z', updated_at: '2026-10-01T10:00:00Z', author: { sub: 'kc-moi', name: 'moi' }, similar_to: 'Les Sillons', track_title: 'Titre découvert', size: 380e6 }],
        removal_candidates: etat.supprimes.includes(albumDecouvert.id) ? [] : [{ addition: 'f0000000-0000-0000-0000-00000000000a', album: albumDecouvert, size: 380e6, author: { sub: 'kc-moi', name: 'moi' }, similar_to: 'Les Sillons', track_title: 'Titre découvert', skips: 3, available_since: '2026-10-01T10:00:00Z' }],
      })
      case '/albums/recent': return json(route, { items: [album], next_cursor: null })
      case '/artists': return json(route, { items: [artiste], next_cursor: null })
      case `/artists/${artiste.id}`: return json(route, { ...artiste, albums: [album] })
      case `/albums/${album.id}`: return json(route, { ...album, tracks: pistes })
      case '/search': return json(route, url.searchParams.get('q')?.toLowerCase().includes('morceau') ? { artists: [], albums: [], tracks: pistes } : { artists: [artiste], albums: [album], tracks: [] })
      default: return json(route, { error: 'not_found', message: 'introuvable' }, 404)
    }
  })
  return { appelsEstompe, lectures, etat }
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

const CAPTURES = process.env.SILLON_CAPTURES
async function capture(page: Page, nom: string, attendre?: () => Promise<void>) {
  if (!CAPTURES) return
  const avant = page.viewportSize()
  for (const [l, h, suffixe] of [[1440, 900, 'large'], [390, 844, 'telephone']] as const) {
    await page.setViewportSize({ width: l, height: h })
    await attendre?.()
    await page.evaluate(() => document.fonts.ready)
    await page.waitForFunction(() => document.getAnimations().every((a) => a.playState !== 'running'))
    await page.screenshot({ path: `${CAPTURES}/${nom}-${suffixe}.png` })
  }
  if (avant) await page.setViewportSize(avant)
}

const ligne = (page: Page, titre: string) => page.locator('.sillon-ligne', { has: page.getByRole('button', { name: new RegExp(`^(Lire|En cours) : ${titre},`) }) })
const ordrePlaylist = (page: Page) => page.locator('.sillon-ligne .sillon-piste-t strong').allTextContents()

test('C1 : playlist créée, album ajouté depuis son menu, réordonnée au clavier et au glisser, ordre gardé au rechargement', async ({ page }) => {
  const s = await simuler(page, { estompe: false, sillon: true })
  await page.goto(`/#/musique/album/${album.id}`)
  await expect(page.getByRole('heading', { name: 'Premier Pressage', level: 1 })).toBeVisible()
  await page.getByRole('button', { name: 'Nouvelle playlist' }).click()
  await page.getByLabel('Nom de la playlist').fill('Route du soir')
  await page.getByRole('button', { name: 'Créer' }).click()
  await expect(page.getByRole('heading', { name: 'Route du soir', level: 1 })).toBeVisible()
  await expect(page.getByText('Cette playlist est vide')).toBeVisible()

  await page.goto(`/#/musique/album/${album.id}`)
  await page.getByRole('button', { name: 'Plus d’actions sur l’album' }).click()
  await page.getByRole('menuitem', { name: 'Ajouter à une playlist…' }).or(page.getByRole('button', { name: 'Ajouter à une playlist…' })).click()
  await page.getByRole('button', { name: /Route du soir/ }).click()
  await expect.poll(() => s.etat.playlists[0]?.items.length).toBe(4)

  await page.getByRole('link', { name: 'Route du soir' }).click()
  await expect(page.getByRole('heading', { name: 'Route du soir', level: 1 })).toBeVisible()
  expect(await ordrePlaylist(page)).toEqual(['Morceau 1', 'Morceau 2', 'Morceau 3', 'Morceau 4'])
  await ligne(page, 'Morceau 1').getByRole('button', { name: /Morceau 1/ }).first().focus()
  await page.keyboard.press('Alt+ArrowDown')
  await expect.poll(() => ordrePlaylist(page)).toEqual(['Morceau 2', 'Morceau 1', 'Morceau 3', 'Morceau 4'])
  await expect(ligne(page, 'Morceau 1').locator('.sillon-piste')).toBeFocused()
  await ligne(page, 'Morceau 4').dragTo(ligne(page, 'Morceau 2'))
  await expect.poll(() => ordrePlaylist(page)).toEqual(['Morceau 4', 'Morceau 2', 'Morceau 1', 'Morceau 3'])
  await capture(page, 'playlist')

  await page.reload()
  await expect(page.getByRole('heading', { name: 'Route du soir', level: 1 })).toBeVisible()
  await expect.poll(() => ordrePlaylist(page)).toEqual(['Morceau 4', 'Morceau 2', 'Morceau 1', 'Morceau 3'])
  await page.getByRole('button', { name: 'Supprimer la playlist' }).click()
  await page.getByRole('button', { name: 'Supprimer', exact: true }).click()
  await expect(page.getByRole('link', { name: 'Route du soir' })).toHaveCount(0)
  expect(s.etat.playlists).toEqual([])
})

test('C2 : la lecture interrompue est proposée à la reprise à la même piste et à la même position', async ({ page }) => {
  const s = await simuler(page, { estompe: false, sillon: true })
  await page.goto(`/#/musique/album/${album.id}`)
  await page.getByRole('button', { name: 'Lire : Morceau 3, Les Sillons' }).click()
  await expect(mini(page)).toContainText('Morceau 3')
  const position = mini(page).getByLabel('Position dans le morceau')
  await expect.poll(async () => Number(await position.inputValue())).toBeGreaterThanOrEqual(3)
  await mini(page).getByRole('button', { name: 'Pause' }).click()
  await expect.poll(() => s.etat.file.position).toBeGreaterThanOrEqual(2.5)
  const garde = s.etat.file.position
  expect(s.etat.file.items[s.etat.file.index].track.title).toBe('Morceau 3')

  await page.goto('/#/musique')
  await page.reload()
  await expect(page.getByRole('button', { name: 'Reprendre Morceau 3, Les Sillons' })).toBeVisible()
  await expect(mini(page)).toContainText('Morceau 3')
  await expect(mini(page).getByRole('button', { name: 'Lecture' })).toBeVisible()
  expect(Number(await position.inputValue())).toBe(Math.round(garde))
  await page.getByRole('button', { name: 'Reprendre Morceau 3, Les Sillons' }).click()
  await expect(mini(page).getByRole('button', { name: 'Pause' })).toBeVisible()
  await expect.poll(async () => Number(await position.inputValue())).toBeGreaterThan(Math.round(garde))
  await mini(page).getByRole('button', { name: 'Suivant' }).click()
  await expect(mini(page)).toContainText('Morceau 4')
  await expect.poll(() => s.etat.ecoutes.at(-1)).toMatchObject({ track: pistes[2].id, ended_by: 'skip', source: { type: 'album', id: album.id } })
})

test('C3 : aléatoire avec suggestions et découverte marquées distinctement, répétition, suggestion ajoutée à la playlist ou écartée', async ({ page }) => {
  const srv = serveurSillon()
  const longues = [pistes[2], pistes[3]]
  srv.etat.playlists.push({ id: 'd0000000-0000-0000-0000-0000000000aa', name: 'Ma playlist', description: null, created_at: '2026-11-16T10:00:00Z', updated_at: '2026-11-16T10:00:00Z', items: longues.map((t, i) => ({ id: `d0000000-0000-0000-0000-0000000001${i}0`, position: i, added_at: '', track: t })) })
  await simuler(page, { estompe: false, sillon: true }, srv)
  await page.goto('/#/musique/queue')
  await page.getByRole('radio', { name: 'Fréquentes' }).click()
  await expect.poll(() => srv.etat.file.suggestions).toBe('frequent')
  await page.getByRole('link', { name: 'Ma playlist' }).click()
  await page.getByRole('button', { name: 'Aléatoire' }).click()
  await expect(mini(page)).toBeVisible()
  await expect.poll(() => srv.etat.file.shuffle).toBe(true)
  await mini(page).getByRole('link', { name: 'File de lecture' }).click()
  const suggeree = page.locator('.sillon-file-ligne.marque:not(.decouverte)')
  await expect(suggeree).toContainText('Titre suggéré')
  await expect(suggeree.locator('.v-tag')).toHaveText('Suggestion')
  await expect(page.locator('.sillon-file-ligne.decouverte')).toHaveCount(0)

  const autre = srv.etat.file.items[1].track
  await page.getByRole('button', { name: `Lire : ${autre.title}, ${autre.artist.name}` }).click()
  await expect.poll(() => srv.etat.file.index).toBe(1)
  const decouverteLigne = page.locator('.sillon-file-ligne.decouverte')
  await expect(decouverteLigne).toContainText('Titre découvert')
  await expect(decouverteLigne.locator('.v-tag.info')).toHaveText('Découverte')
  await expect(page.getByRole('button', { name: 'Lire : Titre découvert, Inconnus (découverte)' })).toBeVisible()
  await expect(suggeree.locator('.v-tag.rose')).toHaveText('Suggestion')

  const repeter = page.locator('.sillon-reglages').getByRole('button', { name: /^Répétition/ })
  await expect(repeter).toHaveText('Sans répétition')
  await repeter.click()
  await expect.poll(() => srv.etat.file.repeat).toBe('all')
  await expect(repeter).toHaveText('Répéter la file')
  await expect(repeter).toHaveAttribute('aria-pressed', 'true')
  await expect(mini(page).getByRole('button', { name: 'Répétition : Répéter la file' })).toHaveAttribute('aria-pressed', 'true')
  await capture(page, 'file-suggestion-decouverte')

  await suggeree.getByRole('button', { name: 'Ajouter à « Ma playlist »' }).click()
  await expect(suggeree.getByText('Ajouté à « Ma playlist »')).toBeVisible()
  expect(srv.etat.playlists[0].items.at(-1)?.track.id).toBe(suggestion.id)
  await suggeree.getByRole('button', { name: 'Ne plus proposer' }).click()
  await expect(page.locator('.sillon-file-ligne.marque:not(.decouverte)')).toHaveCount(0)
  expect(srv.etat.ecartees).toEqual([suggestion.id])
  expect(srv.etat.file.items.some((e) => e.track.id === suggestion.id)).toBe(false)
  await expect(decouverteLigne).toBeVisible()
})

test('Titres similaires depuis le menu d’une piste, et like dans la bande du mini-lecteur au téléphone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  const s = await simuler(page, { estompe: false, sillon: true })
  const bande = page.locator('.sillon-lect.bande')
  await page.goto(`/#/musique/album/${album.id}`)
  await ligne(page, 'Morceau 1').click({ button: 'right' })
  await page.getByRole('menuitem', { name: 'Titres similaires' }).click()
  await expect(bande).toContainText('Morceau 4')
  await expect.poll(() => s.etat.file.items.map((e) => e.track.title)).toEqual(['Morceau 4', 'Morceau 3'])
  await expect(bande.getByRole('button', { name: 'Pause' })).toBeVisible()
  await bande.getByRole('button', { name: 'Aimer Morceau 4' }).click()
  await expect(bande.getByRole('button', { name: 'Ne plus aimer Morceau 4' })).toHaveAttribute('aria-pressed', 'true')
  await expect.poll(() => s.etat.likes.tracks).toEqual([pistes[3].id])
  await capture(page, 'mini-telephone-like')
})

test('C4 : sans sillon:add ni sillon:admin, ni recherche d’absents, ni ajout, ni administration', async ({ page }) => {
  await simuler(page, { estompe: false, sillon: true })
  await page.goto('/#/musique')
  await expect(page.getByRole('link', { name: 'Historique' })).toBeVisible()
  await expect(page.getByRole('link', { name: 'Ajouter de la musique' })).toHaveCount(0)
  await expect(page.getByRole('link', { name: 'Mes ajouts' })).toHaveCount(0)
  await expect(page.getByRole('link', { name: 'Administration' })).toHaveCount(0)
  await page.goto('/#/musique/add')
  await expect(page.getByRole('heading', { name: 'Ajouts récents' })).toBeVisible()
  await expect(page.getByLabel('Chercher un artiste ou un album à ajouter')).toHaveCount(0)
  await page.goto('/#/musique/admin')
  await expect(page.getByRole('heading', { name: 'Administration' })).toHaveCount(0)
  await page.goto('/#/musique/queue')
  await expect(page.getByRole('switch')).toHaveCount(0)
})

test('C5 : ajout refusé pour disque plein, avec l’occupation ; « Mes ajouts » et leur état', async ({ page }) => {
  await simuler(page, { estompe: false, sillon: true, add: true })
  await page.goto('/#/musique')
  await page.getByRole('link', { name: 'Ajouter de la musique' }).click()
  await page.getByLabel('Chercher un artiste ou un album à ajouter').fill('absents')
  await page.getByRole('button', { name: 'Chercher' }).click()
  await expect(page.getByText('Dans la bibliothèque')).toBeVisible()
  await page.getByRole('button', { name: 'Ajouter l’album Trop Lourd' }).click()
  await expect(page.getByRole('alert')).toHaveText('Disque plein : 490 Go sur 500 Go occupés, 10 Go libres ; cet ajout demande environ 20 Go.')
  await capture(page, 'ajout-disque-plein')
  await page.getByRole('button', { name: 'Ajouter l’artiste Les Absents' }).click()
  await expect(page.getByText('En cours d’ajout')).toBeVisible()
  await page.getByRole('link', { name: 'Mes ajouts' }).click()
  await expect(page.locator('.sillon-ajout', { hasText: 'Introuvable' })).toContainText('Échec')
  await expect(page.locator('.sillon-ajout', { hasText: 'En Route' })).toContainText('Téléchargement')
  await expect(page.getByRole('button', { name: 'Relancer' })).toHaveCount(0)
  await capture(page, 'mes-ajouts')
})

test('Images de la recherche d’ajout : pochette et portrait servis par Sillon, vignette de repli sans image ou en échec', async ({ page }) => {
  await simuler(page, { estompe: false, sillon: true, add: true })
  const demandes: string[] = []
  page.on('request', (r) => { if (r.resourceType() === 'image' || r.url().includes('/additions/images/')) demandes.push(r.url()) })
  await page.goto('/#/musique/add')
  await page.getByLabel('Chercher un artiste ou un album à ajouter').fill('absents')
  await page.getByRole('button', { name: 'Chercher' }).click()
  const ligne = (texte: string) => page.locator('.sillon-absent-l').filter({ has: page.locator('strong', { hasText: new RegExp(`^${texte}$`) }) })
  for (const texte of ['Trop Lourd', 'Les Absents']) {
    await expect(ligne(texte).locator('img')).toHaveAttribute('src', /^blob:/)
    await expect.poll(() => ligne(texte).locator('img').evaluate((i: HTMLImageElement) => i.complete && i.naturalWidth)).toBe(250)
  }
  await expect(ligne('Les Absents').locator('.sillon-rond img')).toHaveCount(1)
  for (const texte of ['Sans Pochette', 'Pochette Perdue', 'Déjà Là', 'Sans Visage']) {
    await expect(ligne(texte).locator('.sillon-flat')).toBeVisible()
    await expect(ligne(texte).locator('img')).toHaveCount(0)
  }
  expect(demandes.filter((u) => u.includes('/additions/images/')).sort()).toEqual([
    `${SILLON}/additions/images/a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0?size=small`,
    `${SILLON}/additions/images/b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1?size=small`,
    `${SILLON}/additions/images/${IMAGE_CASSEE}?size=small`,
  ])
  expect(demandes.filter((u) => !u.startsWith('blob:') && ![SILLON, ORIGINE].includes(new URL(u).origin))).toEqual([])
  await capture(page, 'ajout-images')
  await page.getByRole('link', { name: 'Mes ajouts' }).click()
  await expect(page.locator('.sillon-ajout', { hasText: 'En Route' }).locator('img')).toHaveAttribute('src', /^blob:/)
  await expect(page.locator('.sillon-ajout', { hasText: 'Introuvable' }).locator('.sillon-flat')).toBeVisible()
  await capture(page, 'mes-ajouts-images')
})

test('Administration : occupation, relance d’un échec, suppression, synchronisation, analyse, fichiers écartés, découverte', async ({ page }) => {
  const s = await simuler(page, { estompe: false, sillon: true, add: true, admin: true })
  await page.goto('/#/musique/admin')
  await expect(page.getByText('490 Go sur 500 Go')).toBeVisible()
  await capture(page, 'administration')
  await page.locator('.sillon-ajout', { hasText: 'Introuvable' }).getByRole('button', { name: 'Relancer' }).click()
  await expect.poll(() => s.etat.relances).toEqual(['f0000000-0000-0000-0000-000000000001'])
  await expect(page.locator('.sillon-ajout', { hasText: 'Introuvable' })).toContainText('Envoyé')
  await page.getByRole('tab', { name: 'Albums' }).click()
  await page.getByRole('button', { name: 'Supprimer l’album Premier Pressage' }).click()
  await expect(page.getByRole('heading', { name: 'Supprimer l’album « Premier Pressage » ?' })).toBeVisible()
  await page.getByRole('button', { name: 'Annuler' }).click()
  await page.getByRole('tab', { name: 'Synchronisation' }).click()
  await expect(page.getByText('Lidarr configuré')).toBeVisible()
  await page.getByRole('tab', { name: 'Analyse' }).click()
  await expect(page.getByText('décodage impossible')).toBeVisible()
  await page.getByRole('tab', { name: 'Fichiers écartés' }).click()
  await expect(page.getByText('pochette.exe')).toBeVisible()
  await page.getByRole('tab', { name: 'Découverte' }).click()
  await expect(page.getByText('12 Go sur 50 Go')).toBeVisible()
  await expect(page.getByText('Active', { exact: true })).toBeVisible()
  await expect(page.locator('.sillon-admin-liste li', { hasText: 'Titre découvert' }).last()).toContainText('Disponible')
  await capture(page, 'administration-decouverte')
  await page.getByRole('button', { name: 'Supprimer l’album Lointain' }).click()
  await page.getByRole('button', { name: 'Supprimer', exact: true }).click()
  await expect.poll(() => s.etat.supprimes).toEqual([albumDecouvert.id])
  await expect(page.getByText('Aucune découverte à supprimer.')).toBeVisible()
})

test('C6 : une URL d’album ouverte à froid affiche l’album après connexion', async ({ page }) => {
  let connecte = false
  await simuler(page, { estompe: true, sillon: true, connecte: () => connecte })
  await page.goto(`/#/musique/album/${album.id}`)
  const lien = page.getByRole('link', { name: 'Se connecter' })
  await expect(lien).toBeVisible()
  expect(decodeURIComponent(new URL((await lien.getAttribute('href'))!).searchParams.get('redirect')!)).toBe(`${ORIGINE}/#/musique/album/${album.id}`)
  connecte = true
  await page.goto(`/#/musique/album/${album.id}`)
  await page.reload()
  await expect(page.getByRole('heading', { name: 'Premier Pressage', level: 1 })).toBeVisible()
})

test('Like partout et « Titres likés »', async ({ page }) => {
  const s = await simuler(page, { estompe: false, sillon: true })
  await page.goto(`/#/musique/album/${album.id}`)
  await page.getByRole('button', { name: 'Aimer Morceau 2' }).click()
  await expect(page.getByRole('button', { name: 'Ne plus aimer Morceau 2' })).toHaveAttribute('aria-pressed', 'true')
  await page.getByRole('button', { name: 'Aimer Premier Pressage' }).click()
  await expect.poll(() => s.etat.likes).toEqual({ tracks: [pistes[1].id], albums: [album.id], artists: [] })
  await page.getByRole('link', { name: 'Titres likés' }).click()
  await expect(page.getByRole('heading', { name: 'Titres likés', level: 1 })).toBeVisible()
  expect(await ordrePlaylist(page)).toEqual(['Morceau 2'])
})
