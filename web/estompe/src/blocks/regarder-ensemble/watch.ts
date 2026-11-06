import type { ChannelKind, SessionKind, WatchState } from '../../lib/types'

/** Type de salon où se lance une session : les films dans un salon cinéma, le reste dans un salon vocal. */
export const roomKind = (kind: SessionKind): ChannelKind => (kind === 'cinema' ? 'cinema' : 'voice')

/** Salon choisi sans poser de question : celui où l'on est déjà, ou le seul possible si l'on n'est connecté à aucun autre salon vocal. */
export function autoRoom<R extends { id: string }>(rooms: R[], connected: string | null): R | null {
  return rooms.find((r) => r.id === connected) ?? (rooms.length === 1 && !connected ? rooms[0] : null)
}

/** Décalage entre l'horloge du serveur et la nôtre (ms) : à ajouter à `Date.now()` pour obtenir l'heure du serveur. */
export const clockOffset = (serverNow: string, clientNow = Date.now()) => Date.parse(serverNow) - clientNow

/** Position attendue (secondes) : position enregistrée + temps écoulé depuis l'horodatage du serveur, à la vitesse de lecture. */
export function expectedPosition(s: Pick<WatchState, 'playing' | 'pos' | 'rate' | 'at'>, offset: number, clientNow = Date.now()): number {
  if (!s.playing) return s.pos
  return Math.max(0, s.pos + Math.max(0, (clientNow + offset - Date.parse(s.at)) / 1000) * s.rate)
}

/** Un état plus ancien que celui déjà appliqué (évènements croisés) ne doit jamais l'écraser ; à égalité, c'est le même état de lecture. */
export const stale = (cur: { rev: number } | undefined, next: { rev: number }) => !!cur && next.rev < cur.rev

export const SEEK_ABOVE = 1 // écart (s) au-delà duquel on se place directement à la bonne position
export const IGNORE_BELOW = 0.15 // écart (s) toléré
const NUDGE = 0.25 // YouTube n'accepte que les vitesses par pas de 0,25 : on accélère ou ralentit d'un cran, le temps de rattraper

export type Fix = { seek: number } | { rate: number; ms: number } | null

/**
 * Recalage d'un lecteur dont la position dépasse l'attendue de `drift` secondes (négatif : en retard).
 * Petit écart : un cran de vitesse pendant la durée qui comble l'écart ; grand écart : saut direct ; le lecteur au milieu d'un saut
 * ou d'un chargement n'est pas corrigé (appelant).
 */
export function correction(drift: number, expected: number, rate: number): Fix {
  const d = Math.abs(drift)
  if (d >= SEEK_ABOVE) return { seek: expected }
  if (d < IGNORE_BELOW) return null
  const faster = drift < 0
  const target = faster ? rate + NUDGE : rate - NUDGE
  if (target < 0.25 || target > 2) return { seek: expected }
  return { rate: target, ms: Math.round((d / NUDGE) * 1000) }
}

/** Message affiché pour un code d'erreur du lecteur YouTube (IFrame API). */
export function playerError(code: number): string {
  switch (code) {
    case 2: return 'Lien de vidéo invalide.'
    case 5: return 'Cette vidéo ne peut pas être lue par le lecteur intégré.'
    case 100: return 'Vidéo introuvable : supprimée ou privée.'
    case 101:
    case 150: return "Le propriétaire de la vidéo n'autorise pas sa lecture dans une autre application."
    case 153: return "YouTube refuse la lecture intégrée ici (origine ou référent absent)."
    default: return 'Lecture impossible.'
  }
}

/** Texte discret de la dernière action (« X a mis en pause »), vide pour les actions automatiques. */
export function lastAction(s: Pick<WatchState, 'act' | 'by'>): string {
  const who = s.by || 'Quelqu’un'
  switch (s.act) {
    case 'play': return `${who} a relancé la lecture`
    case 'pause': return `${who} a mis en pause`
    case 'seek': return `${who} a changé la position`
    case 'rate': return `${who} a changé la vitesse`
    case 'video': return `${who} a changé de vidéo`
    default: return ''
  }
}
