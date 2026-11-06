const timeFmt = new Intl.DateTimeFormat('fr-FR', { hour: '2-digit', minute: '2-digit' })
const dayFmt = new Intl.DateTimeFormat('fr-FR', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })

/** Compte système « Estompe » (migration 014) : seul bot, non usurpable (pseudo unique, id fixé en base). */
export const BOT_ID = '00000000-0000-0000-0000-000000000001'
export const isBot = (userId: string) => userId === BOT_ID

export const timeOf = (iso: string) => timeFmt.format(new Date(iso))
export const dayOf = (iso: string) => dayFmt.format(new Date(iso))
/** Clé de jour locale, pour les séparateurs. */
export const dayKey = (iso: string) => {
  const d = new Date(iso)
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`
}

export function formatSize(n: number): string {
  if (n < 1024) return `${n} o`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} Ko`
  return `${(n / 1024 / 1024).toFixed(1)} Mo`
}

export const initials = (pseudo: string) => pseudo.slice(0, 2).toUpperCase()

/** Un message prolonge le précédent s'il a le même auteur, le même jour, à moins de 5 minutes. */
export function continues(prev: { author: { id: string }; created_at: string } | undefined, m: { author: { id: string }; created_at: string }): boolean {
  if (!prev || prev.author.id !== m.author.id || dayKey(prev.created_at) !== dayKey(m.created_at)) return false
  return new Date(m.created_at).getTime() - new Date(prev.created_at).getTime() < 5 * 60_000
}

export const STATUS_LABEL = { off: 'Hors ligne', online: 'En ligne', idle: 'Absent', dnd: 'Ne pas déranger' } as const

/** Nombre affiché dans une pastille : plafonné à 99+. */
export const badge = (n: number) => (n > 99 ? '99+' : String(n))

/** Durée en secondes : « 42 s », « 7:05 » (minutes:secondes) ou « 2 h 05 ». */
export function formatDuration(sec: number): string {
  const s = Math.max(0, Math.floor(sec))
  if (s < 60) return `${s} s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}:${String(s % 60).padStart(2, '0')}`
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')}`
}

/** Temps cumulé en vocal, à la minute près (« 3 h 05 », « 12 min »). */
export function formatVoiceTotal(sec: number): string {
  const m = Math.floor(sec / 60)
  return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')}`
}

/** Temps vocal d'un membre : cumulé (la session en cours avance depuis le dernier relevé `statsAt`) et durée de la session en cours (null hors vocal). */
export function voiceTimes(total: number | undefined, statsAt: number, since: number | null, now: number): { total: number; current: number | null } {
  const live = since === null ? 0 : Math.max(0, (now - Math.max(statsAt, since)) / 1000)
  return { total: (total ?? 0) + live, current: since === null ? null : Math.max(0, (now - since) / 1000) }
}

/** Depuis quand le salon est occupé : l'arrivée la plus ancienne, ou null si personne (ou heure inconnue). */
export function oldestSince(ps: { since?: string }[] | undefined): number | null {
  const t = (ps ?? []).map((p) => Date.parse(p.since ?? '')).filter((n) => !Number.isNaN(n))
  return t.length ? Math.min(...t) : null
}

/** Jeu d'après le nom d'un replay (`{jeu}_AAAA-MM-JJ_HH-MM-SS.mp4`, même règle que `game_of` côté Rust) ; vide pour un autre nom. */
export function gameOfName(name: string): string {
  const m = /^(.+)_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}\.mp4$/i.exec(name)
  return m ? m[1].replace(/_/g, ' ') : ''
}
