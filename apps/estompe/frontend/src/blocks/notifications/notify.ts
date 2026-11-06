import { prefs } from '../../lib/prefs.svelte'
import type { ServerNotification } from '../../lib/types'
import { platform } from '../../lib/platform'

let ctx: AudioContext | null = null

/** Petit bip généré à la volée (aucun fichier audio à charger). */
function beep(freqs = [880], step = 0.18, level = 0.08) {
  try {
    ctx ??= new AudioContext()
    const c = ctx
    freqs.forEach((f, i) => {
      const t = c.currentTime + i * step
      const o = c.createOscillator()
      const g = c.createGain()
      o.type = 'sine'
      o.frequency.value = f
      g.gain.setValueAtTime(Math.max(level, 0.0002), t)
      g.gain.exponentialRampToValueAtTime(0.0001, t + step)
      o.connect(g).connect(c.destination)
      o.start(t)
      o.stop(t + step + 0.02)
    })
  } catch { /* audio indisponible */ }
}

/** Sons proposés pour l'arrivée et le départ dans le vocal (fréquences en Hz, jouées à la suite). */
export const SOUND_PRESETS: Record<string, { label: string; notes: number[] }> = {
  rise: { label: 'Montée', notes: [660, 880] },
  fall: { label: 'Descente', notes: [880, 660] },
  chime: { label: 'Carillon', notes: [784, 988, 1319] },
  pop: { label: 'Pop', notes: [520] },
  double: { label: 'Double bip', notes: [700, 700] },
}

let custom: HTMLAudioElement | null = null

/** Joue un son du vocal selon les réglages (preset, fichier personnalisé ou rien). `choice` permet l'essai depuis les paramètres. */
export function playTone(kind: 'join' | 'leave', choice?: string) {
  const c = choice ?? (kind === 'join' ? prefs.joinSound : prefs.leaveSound)
  if (c === 'off') return
  const vol = Math.min(1, Math.max(0, prefs.soundVolume / 100))
  if (c === 'custom') {
    const src = kind === 'join' ? prefs.customJoin : prefs.customLeave
    if (!src) return
    try {
      custom?.pause()
      custom = new Audio(src)
      custom.volume = vol
      void custom.play().catch(() => {})
    } catch { /* audio indisponible */ }
    return
  }
  const notes = SOUND_PRESETS[c]?.notes ?? SOUND_PRESETS[kind === 'join' ? 'rise' : 'fall'].notes
  beep(notes, 0.1, 0.2 * vol)
}

let tap: ((n: ServerNotification) => void) | null = null
/** App Windows : l'overlay des messages reçoit les notifications affichées. */
export const tapNotifications = (f: (n: ServerNotification) => void) => { tap = f }

/** Notification décidée par le serveur (réglages, mentions, ne pas déranger) et reçue en direct par l'app ouverte. `icon` : URL de l'icône du serveur concerné. */
export function showNotification(n: ServerNotification, watching: boolean, icon = '') {
  if (watching && document.hasFocus()) return
  tap?.(n)
  if (!n.silent) beep()
  if (!prefs.desktopNotifications) return
  const href = n.url.replace(/^\//, '') // « /#/dm/x » → « #/dm/x »
  void platform.notifications.show({ title: n.title, body: n.body, href, conv: n.msg ? (n.conv ?? '') : '', tag: n.tag, icon }).catch((e) => console.warn('notification impossible', e))
}
