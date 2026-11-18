import type { Page, Route } from '@playwright/test'

export const ESTOMPE = 'http://estompe.test'
export const ENTRACTE = 'http://entracte.test'
export const SILLON = 'http://sillon.test'
export const SABLIER = 'http://sablier.test'
export const AUJOURDHUI = '2026-11-18'
export const PREMIER_JOUR = '2026-09-03'
export const SERVEUR = 's0000000-0000-0000-0000-000000000001'
export const SALONS = { general: 'c0000000-0000-0000-0000-000000000001', cinema: 'c0000000-0000-0000-0000-000000000002', canape: 'c0000000-0000-0000-0000-000000000003', disparu: 'c0000000-0000-0000-0000-00000000dead' }

export interface Profil {
  estompe?: boolean
  acces?: boolean
  admin?: boolean
  entracte?: boolean
  sillon?: boolean
  arrete?: boolean
  aucuneActivite?: boolean
  sectionEntracte?: boolean
}

type Corps = Record<string, unknown>
const H = 3600

const jourPlus = (jour: string, n: number) => new Date(Date.parse(`${jour}T00:00:00Z`) + n * 86_400_000).toISOString().slice(0, 10)
function plage(periode: string, date: string): [string, string] {
  const d = new Date(`${date}T00:00:00Z`)
  if (periode === 'week') { const lundi = jourPlus(date, -((d.getUTCDay() + 6) % 7)); return [lundi, jourPlus(lundi, 6)] }
  if (periode === 'month') return [`${date.slice(0, 7)}-01`, new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 0)).toISOString().slice(0, 10)]
  if (periode === 'year') return [`${date.slice(0, 4)}-01-01`, `${date.slice(0, 4)}-12-31`]
  return [PREMIER_JOUR, date]
}

const NOVEMBRE = [40, 22, 55, 0, 30, 48, 70, 100, 35, 0, 28, 44, 61, 82, 38, 52, 47, 20]
const usageVide = (periode: string, from: string, to: string, premier: string | null) => ({ period: periode, from, to, seconds: 0, by_day: [], by_section: [], by_platform: [], longest_day: null, active_days: 0, first_day: premier })
const entracteVide = (periode: string, from: string, to: string) => ({ access: true, period: periode, from, to, seconds: 0, movies_watched: 0, episodes_watched: 0, series_completed: 0, longest_session: 0, biggest_day: null, most_episodes_in_a_row: 0, abandoned: 0, intros_skipped: 0, cinema_seconds: 0, cinema_sessions: 0 })
const sillonVide = (periode: string, from: string, to: string) => ({ access: true, period: periode, from, to, seconds: 0, tracks_played: 0, distinct_tracks: 0 })
const messagerieVide = (periode: string, from: string, to: string) => ({ period: periode, from, to, total: 0, private: 0, by_channel: [], top_channel: null, busiest_day: null, reactions_given: 0, reactions_received: 0, top_emoji: null, files: 0, voice_seconds: 0, private_voice_seconds: 0, longest_call: 0, top_voice_channel: null, screen_share_seconds: 0 })
const salon = (channel: string, messages: number) => ({ server: SERVEUR, channel, messages })

function jeu(cle: string, periode: string, from: string, to: string) {
  const u = usageVide(periode, from, to, PREMIER_JOUR)
  const e = entracteVide(periode, from, to)
  const s = sillonVide(periode, from, to)
  const m = messagerieVide(periode, from, to)
  if (cle === 'month:2026-11') {
    const total = NOVEMBRE.reduce((a, b) => a + b, 0)
    Object.assign(u, {
      seconds: 31 * H + 20 * 60, active_days: 16, longest_day: { day: '2026-11-08', seconds: 4 * H + 600 },
      by_day: NOVEMBRE.map((n, i) => ({ day: jourPlus(from, i), seconds: Math.round((n / total) * (31 * H + 1200)) })).filter((j) => j.seconds > 0),
      by_section: [{ section: 'entracte', seconds: 51_888 }, { section: 'messagerie', seconds: 34_968 }, { section: 'sillon', seconds: 25_944 }],
      by_platform: [{ platform: 'windows', seconds: 65_424 }, { platform: 'android', seconds: 38_352 }, { platform: 'web', seconds: 9024 }],
    })
    Object.assign(e, { seconds: 22 * H + 300, movies_watched: 4, episodes_watched: 23, series_completed: 1, longest_session: 3 * H + 2400, biggest_day: { day: '2026-11-08', seconds: 5 * H + 900 }, most_episodes_in_a_row: 5, abandoned: 2, intros_skipped: 0, cinema_seconds: 6 * H + 1800, cinema_sessions: 3 })
    Object.assign(s, { seconds: 18 * H + 2700, tracks_played: 312, distinct_tracks: 148 })
    Object.assign(m, {
      total: 642, private: 118, by_channel: [salon(SALONS.general, 281), salon(SALONS.cinema, 139), salon(SALONS.disparu, 64), salon('c-autre', 40)], top_channel: salon(SALONS.general, 281),
      busiest_day: { day: '2026-11-14', messages: 96 }, reactions_given: 87, reactions_received: 112, top_emoji: { emoji: '🎉', count: 41 }, files: 28,
      voice_seconds: 9 * H + 1200, private_voice_seconds: 0, longest_call: 2 * H + 2100, top_voice_channel: { server: SERVEUR, channel: SALONS.canape, seconds: 6 * H }, screen_share_seconds: H + 3000,
    })
  } else if (cle === 'week:2026-11-16') {
    Object.assign(u, { seconds: H + 2100, active_days: 2, longest_day: { day: '2026-11-17', seconds: H }, by_day: [{ day: '2026-11-16', seconds: 2100 }, { day: '2026-11-17', seconds: H }], by_section: [{ section: 'messagerie', seconds: H + 2100 }], by_platform: [{ platform: 'windows', seconds: H + 2100 }] })
    Object.assign(m, { total: 34, private: 0, by_channel: [salon(SALONS.general, 21), salon(SALONS.cinema, 13)], top_channel: salon(SALONS.general, 21), busiest_day: { day: '2026-11-17', messages: 20 }, reactions_given: 6, reactions_received: 4 })
  } else if (cle === 'month:2026-10') {
    Object.assign(u, { seconds: 47 * H, active_days: 27, longest_day: { day: '2026-10-17', seconds: 5 * H + 300 }, by_day: [{ day: '2026-10-17', seconds: 5 * H + 300 }, { day: '2026-10-18', seconds: 2 * H }], by_section: [{ section: 'entracte', seconds: 30 * H }, { section: 'messagerie', seconds: 17 * H }], by_platform: [{ platform: 'windows', seconds: 31 * H }, { platform: 'android', seconds: 16 * H }] })
    Object.assign(e, { seconds: 30 * H, movies_watched: 6, episodes_watched: 31, series_completed: 2, longest_session: 4 * H, biggest_day: { day: '2026-10-04', seconds: 6 * H }, most_episodes_in_a_row: 7, cinema_seconds: 8 * H, cinema_sessions: 4 })
    Object.assign(s, { seconds: 26 * H, tracks_played: 421, distinct_tracks: 203 })
    Object.assign(m, { total: 1108, private: 200, by_channel: [salon(SALONS.general, 700), salon(SALONS.cinema, 208)], top_channel: salon(SALONS.general, 700), busiest_day: { day: '2026-10-31', messages: 142 }, reactions_given: 120, reactions_received: 140, top_emoji: { emoji: '🎉', count: 60 }, files: 30, voice_seconds: 14 * H, longest_call: 3 * H + 600, top_voice_channel: { server: SERVEUR, channel: SALONS.canape, seconds: 9 * H } })
  } else if (cle === 'month:2026-09') {
    Object.assign(u, { seconds: 12 * H, active_days: 9, longest_day: { day: '2026-09-20', seconds: 3 * H }, by_day: [{ day: '2026-09-03', seconds: H }, { day: '2026-09-20', seconds: 3 * H }], by_section: [{ section: 'messagerie', seconds: 12 * H }], by_platform: [{ platform: 'web', seconds: 12 * H }] })
    Object.assign(m, { total: 210, private: 10, by_channel: [salon(SALONS.general, 200)], top_channel: salon(SALONS.general, 200), busiest_day: { day: '2026-09-20', messages: 40 } })
  } else if (cle === 'year:2026' || cle === 'all') {
    Object.assign(u, { seconds: 412 * H, active_days: 68, longest_day: { day: '2026-10-17', seconds: 5 * H + 300 }, by_day: [{ day: '2026-09-20', seconds: 3 * H }, { day: '2026-10-17', seconds: 5 * H + 300 }, { day: '2026-11-08', seconds: 4 * H + 600 }], by_section: [{ section: 'entracte', seconds: 200 * H }, { section: 'messagerie', seconds: 212 * H }], by_platform: [{ platform: 'windows', seconds: 300 * H }, { platform: 'android', seconds: 112 * H }] })
    Object.assign(e, { seconds: 298 * H, movies_watched: 38, episodes_watched: 284, series_completed: 11, longest_session: 4 * H, biggest_day: { day: '2026-10-04', seconds: 6 * H }, most_episodes_in_a_row: 7, abandoned: 5, intros_skipped: 19, cinema_seconds: 60 * H, cinema_sessions: 41 })
    Object.assign(s, { seconds: 213 * H, tracks_played: 4870, distinct_tracks: 1320 })
    Object.assign(m, { total: 8640, private: 900, by_channel: [salon(SALONS.general, 5000), salon(SALONS.cinema, 2740)], top_channel: salon(SALONS.general, 5000), busiest_day: { day: '2026-10-31', messages: 142 }, reactions_given: 1180, reactions_received: 1420, top_emoji: { emoji: '🎉', count: 512 }, files: 310, voice_seconds: 156 * H, longest_call: 3 * H + 600, top_voice_channel: { server: SERVEUR, channel: SALONS.canape, seconds: 90 * H }, screen_share_seconds: 12 * H })
  }
  return { usage: u, entracte: e, sillon: s, messaging: m }
}

const TROPHEES = [
  { id: 2, key: 'encore_un_episode', name: 'Encore un épisode', description: 'Des épisodes vus, un par un.', icon: 'tele', hidden: false, tier: 2, tiers: 4, unlocked_at: '2026-11-14', seen: false, shared: false, progress: { value: 284, next_threshold: 1000, unit: 'count' } },
  { id: 17, key: 'sur_la_lancee', name: 'Sur la lancée', description: 'Des jours d’affilée, toutes activités confondues.', icon: 'etincelle', hidden: false, tier: 2, tiers: 5, unlocked_at: '2026-11-13', seen: false, shared: false, progress: { value: 12, next_threshold: 30, unit: 'days' } },
  { id: 22, key: 'marathon', name: 'Marathon', description: 'Cinq épisodes d’affilée.', icon: 'suivant', hidden: false, tier: 1, tiers: 1, unlocked_at: '2026-11-08', seen: true, shared: false },
  { id: 21, key: 'premier_jour', name: 'Premier jour', description: 'Ta première journée d’activité.', icon: 'etoile', hidden: false, tier: 1, tiers: 1, unlocked_at: '2026-09-03', seen: true, shared: true },
  { id: 28, key: 'signature', name: 'Signature', description: 'Le même emoji, encore et encore.', icon: 'sourire', hidden: true, tier: 1, tiers: 1, unlocked_at: '2026-10-02', seen: true, shared: false },
]
const EN_COURS = [
  { id: 1, key: 'cinephile', name: 'Cinéphile', description: 'Des films vus jusqu’au bout.', icon: 'film', tier: 1, tiers: 4, value: 38, next_threshold: 50, unit: 'count' },
  { id: 6, key: 'platine', name: 'Platine', description: 'Du temps passé à écouter.', icon: 'casque', tier: 2, tiers: 4, value: 412 * H, next_threshold: 500 * H, unit: 'seconds' },
  { id: 9, key: 'pipelette', name: 'Pipelette', description: 'Des messages envoyés.', icon: 'bulle', tier: 2, tiers: 4, value: 8640, next_threshold: 10_000, unit: 'count' },
  { id: 15, key: 'habitue', name: 'Habitué', description: 'Des jours actifs dans un même mois.', icon: 'calendar', tier: 1, tiers: 3, value: 16, next_threshold: 20, unit: 'days' },
  { id: 2, key: 'encore_un_episode', name: 'Encore un épisode', description: 'Des épisodes vus, un par un.', icon: 'tele', tier: 2, tiers: 4, value: 284, next_threshold: 1000, unit: 'count' },
  { id: 26, key: 'appel_fleuve', name: 'Appel fleuve', description: 'Un appel de trois heures.', icon: 'inconnue-du-front', tier: 0, tiers: 1, value: 2 * H + 2100, next_threshold: 3 * H, unit: 'seconds' },
]
const STATS_PARTAGEABLES = [
  { key: 'usage.active_seconds', label: 'Temps actif', domain: 'usage', unit: 'seconds', mois: 31 * H + 1200, total: 412 * H },
  { key: 'streak.global', label: 'Série de jours', domain: 'usage', unit: 'days', mois: 12, total: 41 },
  { key: 'messaging.messages', label: 'Messages envoyés', domain: 'messaging', unit: 'count', mois: 642, total: 8640 },
  { key: 'messaging.voice_seconds', label: 'Temps en vocal', domain: 'messaging', unit: 'seconds', mois: 9 * H + 1200, total: 213 * H },
  { key: 'entracte.movies_watched', label: 'Films vus', domain: 'entracte', unit: 'count', mois: 4, total: 38 },
  { key: 'sillon.seconds', label: 'Temps d’écoute', domain: 'sillon', unit: 'seconds', mois: 18 * H + 2700, total: 213 * H },
]

export function serveurSablier() {
  return {
    trophees: TROPHEES.map((t) => ({ ...t })),
    recaps: [
      { id: 12, kind: 'month', period: '2026-10', created_at: '2026-11-01T03:00:00Z', seen: false },
      { id: 11, kind: 'month', period: '2026-09', created_at: '2026-10-01T03:00:00Z', seen: true },
      { id: 9, kind: 'month', period: '2026-07', created_at: '2026-08-01T03:00:00Z', seen: true },
      { id: 5, kind: 'year', period: '2025', created_at: '2026-01-01T03:00:00Z', seen: true },
    ],
    partages: new Set<string>(),
    appels: [] as { methode: string; chemin: string; corps: Corps }[],
    lectures: [] as string[],
  }
}
export type Sablier = ReturnType<typeof serveurSablier>

function recap(id: number, srv: Sablier, p: Profil) {
  const r = srv.recaps.find((x) => x.id === id)
  if (!r) return null
  const annee = r.kind === 'year'
  const [from, to] = annee ? [`${r.period}-01-01`, `${r.period}-12-31`] : plage('month', `${r.period}-15`)
  const j = jeu(annee ? 'year:2026' : `month:${r.period}`, r.kind, from, to)
  const m = j.messaging
  const highlights: Corps[] = []
  if (j.usage.longest_day) highlights.push({ kind: 'longest_day', value: (j.usage.longest_day as { seconds: number }).seconds, day: (j.usage.longest_day as { day: string }).day })
  if (p.entracte !== false && j.entracte.biggest_day) highlights.push({ kind: 'biggest_day', value: (j.entracte.biggest_day as { seconds: number }).seconds, day: (j.entracte.biggest_day as { day: string }).day }, { kind: 'episodes_in_a_row', value: j.entracte.most_episodes_in_a_row })
  if (m.busiest_day) highlights.push({ kind: 'busiest_day', value: (m.busiest_day as { messages: number }).messages, day: (m.busiest_day as { day: string }).day })
  if (m.top_channel) highlights.push({ kind: 'top_channel', value: (m.top_channel as { messages: number }).messages, server: SERVEUR, channel: SALONS.general })
  if (m.top_emoji) highlights.push({ kind: 'top_emoji', value: (m.top_emoji as { count: number }).count, emoji: (m.top_emoji as { emoji: string }).emoji })
  if (m.longest_call) highlights.push({ kind: 'longest_call', value: m.longest_call })
  const { access: _e, ...entracte } = j.entracte
  const { access: _s, ...sillon } = j.sillon
  return {
    id, kind: r.kind, period: r.period, from, to, usage: j.usage, messaging: m, highlights,
    ...(p.entracte !== false ? { entracte } : {}), ...(p.sillon !== false ? { sillon } : {}),
    trophies: r.period === '2026-10' || annee ? [{ name: 'Signature', description: 'Le même emoji, encore et encore.', icon: 'sourire', tier: 1, unlocked_at: '2026-10-02' }, { name: 'Platine', description: 'Du temps passé à écouter.', icon: 'casque', tier: 2, unlocked_at: '2026-10-11' }, { name: 'Pipelette', description: 'Des messages envoyés.', icon: 'bulle', tier: 2, unlocked_at: '2026-10-20' }] : [],
    streak: { global: r.period === '2026-09' ? 1 : 41, messaging: 41 },
  }
}

export async function simuler(page: Page, p: Profil, srv: Sablier = serveurSablier()) {
  const origine = new URL(page.url() === 'about:blank' ? 'http://localhost:4177' : page.url()).origin
  const cors = { 'access-control-allow-origin': origine, 'access-control-allow-credentials': 'true' }
  const json = (route: Route, body: unknown, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body), headers: cors })
  const vide = (route: Route) => route.fulfill({ status: 204, headers: cors })
  const preflight = (route: Route) => route.fulfill({ status: 204, headers: { ...cors, 'access-control-allow-headers': '*', 'access-control-allow-methods': '*' } })
  const estompe = p.estompe !== false
  const acces = p.acces !== false

  await page.route(`${ESTOMPE}/**`, (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() === 'OPTIONS') return preflight(route)
    switch (url.pathname) {
      case '/me':
        return json(route, estompe
          ? { activated: true, allowed: true, admin: false, rights: ['access'], id: 'me', pseudo: 'moi', sub: 'kc-moi', cinema: false, blocks: ['vocal'] }
          : { activated: false, allowed: false, admin: false, rights: [] })
      case '/unread': return json(route, { channels: [], dms: [], counts: {} })
      case '/me/notify-settings': return json(route, { levels: {}, types: { dm: true, mention: true, channel: true, voice: true }, preview: true, sound: true })
      case '/avatars': case '/statuses': case `/servers/${SERVEUR}/voice`: case `/servers/${SERVEUR}/voice/stats`: return json(route, {})
      case '/ws-ticket': return json(route, { ticket: 't' })
      case '/servers': return json(route, [{ id: SERVEUR, name: 'Le Salon', owner_id: 'me', icon_version: 0 }])
      case `/servers/${SERVEUR}`:
        return json(route, {
          server: { id: SERVEUR, name: 'Le Salon', owner_id: 'me', icon_version: 0 },
          channels: [{ id: SALONS.general, name: 'général', kind: 'text', position: 0 }, { id: SALONS.cinema, name: 'cinéma', kind: 'text', position: 1 }, { id: SALONS.canape, name: 'Canapé', kind: 'voice', position: 2 }],
          members: [{ id: 'me', pseudo: 'moi', online: true }, { id: 'u-camille', pseudo: 'Camille', online: true }],
          roles: [], permissions: [],
        })
      case `/channels/${SALONS.general}/messages`: return json(route, { messages: [], has_more: false })
      default: return json(route, [])
    }
  })
  await page.route(`${ENTRACTE}/**`, (route) => {
    if (route.request().method() === 'OPTIONS') return preflight(route)
    const url = new URL(route.request().url())
    if (url.pathname === '/me') return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: !!p.sectionEntracte, request: false, admin: false }, push: { web: false, fcm: false } })
    if (url.pathname === '/genres') return json(route, { genres: [] })
    return json(route, { items: [], total: 0, last_id: 0 })
  })
  await page.route(`${SILLON}/**`, (route) => {
    if (route.request().method() === 'OPTIONS') return preflight(route)
    return json(route, { error: 'unauthorized', message: 'connexion requise' }, 401)
  })
  await page.route(`${SABLIER}/**`, (route) => {
    const req = route.request()
    const url = new URL(req.url())
    if (p.arrete) return route.abort('connectionrefused')
    if (req.method() === 'OPTIONS') return preflight(route)
    const m = req.method()
    const corps = (m === 'GET' ? {} : (req.postDataJSON() ?? {})) as Corps
    srv.appels.push({ methode: m, chemin: url.pathname, corps })
    if (url.pathname === '/me') return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: acces, admin: !!p.admin, entracte: p.entracte !== false, sillon: p.sillon !== false }, sections: acces ? ['stats', ...(p.admin ? ['admin'] : [])] : [], heartbeat_seconds: 30 })
    if (url.pathname === '/heartbeat') return vide(route)
    if (!acces) return json(route, { error: 'forbidden', message: 'permission sablier:access requise' }, 403)
    const periode = url.searchParams.get('period') ?? 'month'
    const date = url.searchParams.get('date') ?? AUJOURDHUI
    const [from, to] = plage(periode, date)
    const cle = periode === 'all' ? 'all' : periode === 'year' ? `year:${from.slice(0, 4)}` : periode === 'month' ? `month:${from.slice(0, 7)}` : `week:${from}`
    const j = jeu(p.aucuneActivite ? 'rien' : cle, periode, from, to)
    const debloques = p.aucuneActivite ? [] : srv.trophees.filter((t) => (p.entracte !== false || !['encore_un_episode', 'marathon'].includes(t.key)))
    const profil = (id: string) => {
      if (id === 'u-camille') return { month: [{ key: 'messaging.messages', label: 'Messages envoyés', domain: 'messaging', unit: 'count', value: 77 }], total: [{ key: 'messaging.messages', label: 'Messages envoyés', domain: 'messaging', unit: 'count', value: 5120 }], trophies: [{ name: 'Pipelette', description: 'Des messages envoyés.', icon: 'bulle', tier: 3, unlocked_at: '2026-08-02' }] }
      const stats = STATS_PARTAGEABLES.filter((s) => srv.partages.has(s.key))
      return {
        month: stats.map((s) => ({ key: s.key, label: s.label, domain: s.domain, unit: s.unit, value: s.mois })),
        total: stats.map((s) => ({ key: s.key, label: s.label, domain: s.domain, unit: s.unit, value: s.total })),
        trophies: srv.trophees.filter((t) => t.shared).map((t) => ({ name: t.name, description: t.description, icon: t.icon, tier: t.tier, unlocked_at: t.unlocked_at })),
      }
    }
    const reglages = () => ({
      stats: STATS_PARTAGEABLES.filter((s) => (s.domain !== 'entracte' || p.entracte !== false) && (s.domain !== 'sillon' || p.sillon !== false)).map((s) => ({ key: s.key, label: s.label, domain: s.domain, shared: srv.partages.has(s.key) })),
      trophies: debloques.map((t) => ({ id: t.id, name: t.name, icon: t.icon, shared: t.shared })),
    })
    const unRecap = /^\/recaps\/(\d+)$/.exec(url.pathname)
    if (unRecap) {
      const r = recap(Number(unRecap[1]), srv, p)
      if (r) srv.lectures.push(unRecap[1])
      return r ? json(route, r) : json(route, { error: 'not_found', message: 'récap introuvable' }, 404)
    }
    const unProfil = /^\/profiles\/([^/]+)$/.exec(url.pathname)
    if (unProfil) return json(route, profil(decodeURIComponent(unProfil[1])))
    switch (url.pathname) {
      case '/usage': return json(route, p.aucuneActivite ? usageVide(periode, from, to, null) : j.usage)
      case '/entracte': return json(route, p.entracte === false ? { access: false } : j.entracte)
      case '/sillon': return json(route, p.sillon === false ? { access: false } : j.sillon)
      case '/messaging': return json(route, j.messaging)
      case '/trophies': return json(route, { unlocked: debloques, in_progress: p.aucuneActivite ? EN_COURS.filter((t) => t.tier === 0) : EN_COURS.filter((t) => p.sillon !== false || t.key !== 'platine'), hidden_remaining: 4 })
      case '/streaks': return json(route, p.aucuneActivite ? { global: { current: 0, best: 0 }, entracte: null, sillon: null, messaging: { current: 0, best: 0 } } : { global: { current: 12, best: 41 }, entracte: p.entracte === false ? null : { current: 0, best: 9 }, sillon: p.sillon === false ? null : { current: 12, best: 28 }, messaging: { current: 5, best: 41 } })
      case '/unseen': return json(route, { trophies: debloques.filter((t) => !t.seen).length, recaps: p.aucuneActivite ? 0 : srv.recaps.filter((r) => !r.seen).length })
      case '/seen': {
        const marquer = <T extends { id: number; seen: boolean }>(liste: T[], ids: unknown) => { for (const x of liste) if (ids === 'all' || (Array.isArray(ids) && ids.includes(x.id))) x.seen = true }
        marquer(srv.trophees, corps.trophies)
        marquer(srv.recaps, corps.recaps)
        return vide(route)
      }
      case '/recaps': return json(route, p.aucuneActivite ? [] : srv.recaps)
      case '/sharing':
        if (m === 'PUT') {
          for (const [k, v] of Object.entries((corps.stats ?? {}) as Record<string, boolean>)) { if (v) srv.partages.add(k); else srv.partages.delete(k) }
          for (const [k, v] of Object.entries((corps.trophies ?? {}) as Record<string, boolean>)) { const t = srv.trophees.find((x) => String(x.id) === k); if (t) t.shared = v }
        }
        return json(route, reglages())
      default: return json(route, { error: 'not_found', message: 'introuvable' }, 404)
    }
  })
  return srv
}
