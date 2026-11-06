/** Navigateur et PWA : notifications de l'API Notification et Web Push (service worker). */
import { api } from '../../src/lib/api'
import { router } from '../../src/lib/route.svelte'
import type { NotificationDriver, PushDriver } from '../../src/lib/platform'
import { prefs } from '../../src/lib/prefs.svelte'

const b64ToBytes = (s: string) => {
  const raw = atob(s.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(s.length / 4) * 4, '='))
  return Uint8Array.from(raw, (c) => c.charCodeAt(0))
}

const registration = () => navigator.serviceWorker.ready

const supported = () => typeof navigator !== 'undefined' && 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window

const push: PushDriver = {
  supported,
  probeServer: true,
  async enabled() {
    if (!supported()) return false
    try { return !!(await (await registration()).pushManager.getSubscription()) } catch { return false }
  },
  async enable() {
    if (!supported() || (await Notification.requestPermission()) !== 'granted') return false
    const reg = await registration()
    const { public_key } = await api.pushKey()
    const sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: b64ToBytes(public_key) })
    await api.pushSubscribe(sub.toJSON())
    return true
  },
  async disable() {
    const sub = await (await registration()).pushManager.getSubscription()
    if (!sub) return
    await api.pushUnsubscribe(sub.endpoint).catch(() => {})
    await sub.unsubscribe()
  },
  /** Réenregistre l'abonnement (le serveur supprime les invalides ; Safari le révoque parfois) ; en crée un nouveau s'il a disparu. */
  async refresh() {
    if (!supported() || !prefs.desktopNotifications || Notification.permission !== 'granted') return
    try {
      const sub = await (await registration()).pushManager.getSubscription()
      if (sub) await api.pushSubscribe(sub.toJSON())
      else await push.enable()
    } catch { /* serveur sans push ou hors ligne : l'état réel s'affiche dans les Paramètres */ }
  },
}

const iosBrowser = () => /iPhone|iPad|iPod/.test(navigator.userAgent) && !matchMedia('(display-mode: standalone)').matches

export const notifications: NotificationDriver = {
  push,
  testable: true,
  supported: () => typeof Notification !== 'undefined',
  needsGesture: () => typeof Notification !== 'undefined' && Notification.permission === 'default',
  async ask() {
    if (typeof Notification === 'undefined') return false
    if (Notification.permission === 'granted') return true
    return (await Notification.requestPermission()) === 'granted'
  },
  async show(n) {
    if (typeof Notification === 'undefined' || Notification.permission !== 'granted') return
    const w = new Notification(n.title, { body: n.body, tag: n.tag, icon: n.icon || '/icon.svg', silent: true })
    w.onclick = () => {
      window.focus()
      router.go(n.href)
      w.close()
    }
  },
  hint: () => iosBrowser() ? "Sur iPhone et iPad, les notifications fonctionnent seulement depuis l'app installée : dans Safari, touche Partager puis « Sur l'écran d'accueil », et ouvre Estompe depuis l'icône." : '',
}
