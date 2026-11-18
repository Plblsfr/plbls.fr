import type { ClientSablier, Entracte, Messagerie, Partage, Profil, Recap, RecapResume, Series, Sillon, Stats, Trophees, Usage } from './sablier'

export const AUJOURDHUI = '2026-11-18'
export const SERVEUR = 's1'

export const usage = (over: Partial<Usage> = {}): Usage => ({ period: 'month', from: '2026-11-01', to: '2026-11-30', seconds: 0, by_day: [], by_section: [], by_platform: [], longest_day: null, active_days: 0, first_day: '2026-09-03', ...over })
export const entracte = (over: Partial<Entracte> = {}): Entracte => ({ access: true, period: 'month', from: '2026-11-01', to: '2026-11-30', seconds: 0, movies_watched: 0, episodes_watched: 0, series_completed: 0, longest_session: 0, biggest_day: null, most_episodes_in_a_row: 0, abandoned: 0, intros_skipped: 0, cinema_seconds: 0, cinema_sessions: 0, ...over })
export const sillon = (over: Partial<Sillon> = {}): Sillon => ({ access: true, period: 'month', from: '2026-11-01', to: '2026-11-30', seconds: 0, tracks_played: 0, distinct_tracks: 0, ...over })
export const messagerie = (over: Partial<Messagerie> = {}): Messagerie => ({ period: 'month', from: '2026-11-01', to: '2026-11-30', total: 0, private: 0, by_channel: [], top_channel: null, busiest_day: null, reactions_given: 0, reactions_received: 0, top_emoji: null, files: 0, voice_seconds: 0, private_voice_seconds: 0, longest_call: 0, top_voice_channel: null, screen_share_seconds: 0, ...over })

export const statsPleines = (): Stats => ({
  usage: usage({ seconds: 112_800, active_days: 16, longest_day: { day: '2026-11-08', seconds: 15_000 }, by_day: [{ day: '2026-11-01', seconds: 3600 }, { day: '2026-11-08', seconds: 15_000 }], by_section: [{ section: 'entracte', seconds: 60_000 }, { section: 'messagerie', seconds: 52_800 }], by_platform: [{ platform: 'windows', seconds: 100_000 }, { platform: 'android', seconds: 12_800 }] }),
  entracte: entracte({ seconds: 79_500, movies_watched: 4, episodes_watched: 23, series_completed: 1, longest_session: 13_200, biggest_day: { day: '2026-11-08', seconds: 18_900 }, most_episodes_in_a_row: 5, abandoned: 2, intros_skipped: 0, cinema_seconds: 23_400, cinema_sessions: 3 }),
  sillon: sillon({ seconds: 67_500, tracks_played: 312, distinct_tracks: 148 }),
  messaging: messagerie({ total: 642, private: 118, by_channel: [{ server: SERVEUR, channel: 'c1', messages: 281 }, { server: SERVEUR, channel: 'disparu', messages: 139 }], top_channel: { server: SERVEUR, channel: 'c1', messages: 281 }, busiest_day: { day: '2026-11-14', messages: 96 }, reactions_given: 87, reactions_received: 112, top_emoji: { emoji: '🎉', count: 41 }, files: 28, voice_seconds: 33_600, longest_call: 9300, top_voice_channel: { server: SERVEUR, channel: 'v1', seconds: 20_000 }, screen_share_seconds: 6600 }),
})
export const statsVides = (premier: string | null = '2026-09-03'): Stats => ({ usage: usage({ first_day: premier }), entracte: entracte(), sillon: sillon(), messaging: messagerie() })

export const trophees = (): Trophees => ({
  unlocked: [
    { id: 2, key: 'encore_un_episode', name: 'Encore un épisode', description: 'Des épisodes vus, un par un.', icon: 'tele', hidden: false, tier: 2, tiers: 4, unlocked_at: '2026-11-14', seen: false, shared: false, progress: { value: 284, next_threshold: 1000, unit: 'count' } },
    { id: 21, key: 'premier_jour', name: 'Premier jour', description: 'Ta première journée d’activité.', icon: 'etoile', hidden: false, tier: 1, tiers: 1, unlocked_at: '2026-09-03', seen: true, shared: false },
    { id: 28, key: 'signature', name: 'Signature', description: 'Le même emoji, encore et encore.', icon: 'sourire', hidden: true, tier: 1, tiers: 1, unlocked_at: '2026-10-02', seen: true, shared: false },
  ],
  in_progress: [
    { id: 1, key: 'cinephile', name: 'Cinéphile', description: 'Des films vus jusqu’au bout.', icon: 'film', tier: 1, tiers: 4, value: 38, next_threshold: 50, unit: 'count' },
    { id: 2, key: 'encore_un_episode', name: 'Encore un épisode', description: 'Des épisodes vus, un par un.', icon: 'tele', tier: 2, tiers: 4, value: 284, next_threshold: 1000, unit: 'count' },
  ],
  hidden_remaining: 4,
})
export const series = (): Series => ({ global: { current: 12, best: 41 }, entracte: { current: 0, best: 9 }, sillon: null, messaging: { current: 5, best: 41 } })
export const recaps = (): RecapResume[] => [
  { id: 12, kind: 'month', period: '2026-10', created_at: '2026-11-01T03:00:00Z', seen: false },
  { id: 9, kind: 'month', period: '2026-08', created_at: '2026-09-01T03:00:00Z', seen: true },
  { id: 5, kind: 'year', period: '2025', created_at: '2026-01-01T03:00:00Z', seen: true },
]
export const recap = (over: Partial<Recap> = {}): Recap => {
  const s = statsPleines()
  const { access: _e, ...e } = s.entracte as Entracte
  const { access: _s, ...si } = s.sillon as Sillon
  return {
    id: 12, kind: 'month', period: '2026-10', from: '2026-10-01', to: '2026-10-31', usage: s.usage, entracte: e, sillon: si, messaging: s.messaging,
    highlights: [{ kind: 'longest_day', value: 15_000, day: '2026-10-17' }, { kind: 'biggest_day', value: 18_900, day: '2026-10-04' }, { kind: 'episodes_in_a_row', value: 5 }, { kind: 'busiest_day', value: 96, day: '2026-10-31' }, { kind: 'top_channel', value: 281, server: SERVEUR, channel: 'c1' }, { kind: 'top_emoji', value: 41, emoji: '🎉' }],
    trophies: [{ name: 'Signature', description: 'Le même emoji.', icon: 'sourire', tier: 1, unlocked_at: '2026-10-02' }],
    streak: { global: 41, messaging: 41 },
    ...over,
  }
}
export const partage = (): Partage => ({
  stats: [{ key: 'usage.active_seconds', label: 'Temps actif', domain: 'usage', shared: false }, { key: 'messaging.messages', label: 'Messages envoyés', domain: 'messaging', shared: false }, { key: 'entracte.movies_watched', label: 'Films vus', domain: 'entracte', shared: false }],
  trophies: [{ id: 2, name: 'Encore un épisode', icon: 'tele', shared: false }],
})
export const profilVide = (): Profil => ({ month: [], total: [], trophies: [] })

export function fauxClient(over: Partial<ClientSablier> = {}): ClientSablier {
  return {
    me: async () => ({ sub: 'kc-moi', name: 'moi', email: '', permissions: { access: true, admin: false }, sections: ['stats'], heartbeat_seconds: 30 }),
    battre: async () => {},
    usage: async () => statsPleines().usage,
    entracte: async () => statsPleines().entracte,
    sillon: async () => statsPleines().sillon,
    messagerie: async () => statsPleines().messaging,
    trophees: async () => trophees(),
    series: async () => series(),
    nonVus: async () => ({ trophies: 0, recaps: 0 }),
    marquerVus: async () => {},
    recaps: async () => recaps(),
    recap: async () => recap(),
    partage: async () => partage(),
    partager: async () => partage(),
    profil: async () => profilVide(),
    ...over,
  }
}
