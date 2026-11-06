import { platform } from '../../lib/platform'
import { prefs } from '../../lib/prefs.svelte'
import { notify } from '../../lib/ui'

/** État réel du push pour l'interface : « on » seulement si l'abonnement existe et que rien ne le bloque. */
export type PushState = 'on' | 'off' | 'unsupported' | 'denied' | 'unavailable'
export const pushState = (o: { supported: boolean; subscribed: boolean; denied: boolean; serverOff: boolean }): PushState =>
  !o.supported ? 'unsupported' : o.denied ? 'denied' : o.serverOff ? 'unavailable' : o.subscribed ? 'on' : 'off'

/** Une seule fois par appareil, après l'activation par défaut : explique, demande la permission système, puis abonne au push. Un refus désactive l'option (aucune nouvelle demande). */
export async function setupNotifications() {
  if (prefs.notifAsked || !prefs.desktopNotifications) return
  // Un navigateur ignore la demande de permission sans geste de l'utilisateur : on attend le premier clic ou la première touche.
  if (platform.notifications.needsGesture()) {
    const go = () => { removeEventListener('pointerdown', go); removeEventListener('keydown', go); void askAndSubscribe() }
    addEventListener('pointerdown', go)
    addEventListener('keydown', go)
    return
  }
  await askAndSubscribe()
}

async function askAndSubscribe() {
  if (prefs.notifAsked || !prefs.desktopNotifications) return
  prefs.notifAsked = true
  prefs.save()
  notify.info('Estompe va te demander l\'autorisation d\'afficher les notifications, pour ne pas manquer tes messages.')
  if (!(await platform.notifications.ask().catch(() => false))) {
    prefs.desktopNotifications = false
    prefs.save()
    return
  }
  const push = platform.notifications.push
  if (push?.supported() && !(await push.enabled())) await push.enable().catch(() => {}) // serveur sans push : l'état réel s'affiche dans les Paramètres
}
