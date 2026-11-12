/** Activité : jeu en cours, affiché sur son propre profil et, si l'utilisateur y consent, partagé. */
import { api } from '../../lib/api'
import { platform } from '../../lib/platform'
import { prefs } from '../../lib/prefs.svelte'
import { app } from '../../lib/store.svelte'
import { solo } from '../videos/solo.svelte'

const POLL_MS = 5000
/** Jeu réduit ou fermé : il reste affiché ce temps-là après la dernière détection (alt-tab, redémarrage). */
const GRACE_MS = 60_000

/** Nom à afficher : le jeu vu à l'instant, sinon le dernier vu depuis moins de GRACE_MS. */
export function activityName(seen: string | null, last: { name: string; at: number }, now: number): string {
  return seen ?? (now - last.at < GRACE_MS ? last.name : '')
}

/** « 🎵 Écoute … » : le morceau écouté seul, au plus MAX_LEN caractères (limite du serveur). */
export function listeningLabel(title: string, max = 64): string {
  const chars = [...`🎵 Écoute ${title}`]
  return chars.length <= max ? chars.join('') : chars.slice(0, max - 1).join('') + '…'
}

/** Ce que les autres voient : le jeu (si partagé), sinon le morceau écouté seul (si partagé), sinon rien. */
export function sharedActivity(game: string, shareGame: boolean, track: string, shareListening: boolean): string {
  return (shareGame && game) || (shareListening && track ? listeningLabel(track) : '')
}

/** À appeler une fois au démarrage. La détection du jeu en cours est propre à l'app Windows ; l'écoute seul vaut partout. */
export function startActivity() {
  $effect.root(() => {
    $effect(() => {
      if (!platform.foregroundApp || app.phase !== 'ready') return
      const last = { name: '', at: 0 }
      let stop = false
      const poll = async () => {
        try {
          const name = await platform.foregroundApp!()
          if (stop) return
          const now = Date.now()
          if (name) { last.name = name; last.at = now }
          app.localActivity = activityName(name, last, now)
        } catch { /* détection indisponible */ }
      }
      void poll()
      const t = setInterval(poll, POLL_MS)
      return () => { stop = true; clearInterval(t); app.localActivity = '' }
    })
    // Le serveur oublie l'activité à la déconnexion : on la renvoie à chaque (re)connexion.
    let sent = ''
    $effect(() => {
      if (app.connection !== 'open') { sent = ''; return }
      const track = solo.playing && solo.current ? solo.current.title : ''
      const name = sharedActivity(app.localActivity, prefs.shareActivity, track, prefs.shareListening)
      if (name === sent) return
      sent = name
      api.setActivity(name).catch(() => { sent = '' })
    })
  })
}
