import { entracteApi } from '../../lib/api'
import { platform } from '../../lib/platform'
import { freshNotifications } from './entracte'

const POLL_MS = 60_000
const LAST_KEY = 'entracte.notifications.vues'

const b64ToBytes = (s: string) => {
  const raw = atob(s.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(s.length / 4) * 4, '='))
  return Uint8Array.from(raw, (c) => c.charCodeAt(0))
}

const webPush = () => typeof navigator !== 'undefined' && 'serviceWorker' in navigator && typeof window !== 'undefined' && 'PushManager' in window && typeof Notification !== 'undefined'

function readLast(): number | null {
  try { const v = localStorage.getItem(LAST_KEY); return v === null ? null : Number(v) || 0 } catch { return null }
}
function writeLast(n: number) {
  try { localStorage.setItem(LAST_KEY, String(n)) } catch { /* stockage indisponible */ }
}

/** Notifications d'Entracte : Web Push et FCM propres à Entracte ; l'app Windows, sans push, relève `GET /notifications`. */
class EntracteState {
  push = $state<'on' | 'off' | 'unsupported'>('off')
  private timer: ReturnType<typeof setInterval> | null = null
  private run = 0

  async start() {
    this.stop()
    const run = this.run
    await this.registerPush().catch(() => {})
    if (run !== this.run || platform.notifications.push) return
    void this.poll()
    this.timer = setInterval(() => void this.poll(), POLL_MS)
  }

  stop() {
    this.run++
    if (this.timer) clearInterval(this.timer)
    this.timer = null
  }

  /** Réenregistre l'abonnement existant de l'appareil auprès d'Entracte (un navigateur n'en a qu'un, partagé avec Estompe). */
  private async registerPush() {
    const driver = platform.notifications.push
    if (driver?.token) {
      if (!(await driver.enabled())) return
      const token = await driver.token()
      if (token) { await entracteApi.fcmSubscribe(token); this.push = 'on' }
      return
    }
    if (!webPush()) { this.push = 'unsupported'; return }
    if (Notification.permission !== 'granted') return
    const reg = await navigator.serviceWorker.ready
    const sub = await reg.pushManager.getSubscription()
    if (sub) { await entracteApi.pushSubscribe(sub.toJSON()); this.push = 'on' }
  }

  /** Bouton « Recevoir les notifications » : demande la permission, puis abonne l'appareil à Entracte. */
  async enablePush(): Promise<boolean> {
    const driver = platform.notifications.push
    if (driver?.token) {
      if (!(await driver.enabled()) && !(await driver.enable().catch(() => false))) return false
      await this.registerPush()
      return this.push === 'on'
    }
    if (!webPush() || (await Notification.requestPermission()) !== 'granted') return false
    const reg = await navigator.serviceWorker.ready
    let sub = await reg.pushManager.getSubscription()
    if (!sub) {
      const { key } = await entracteApi.pushKey()
      sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: b64ToBytes(key) })
    }
    await entracteApi.pushSubscribe(sub.toJSON())
    this.push = 'on'
    return true
  }

  async poll() {
    const last = readLast()
    try {
      const r = await entracteApi.notifications(last ?? 0)
      if (last === null) { writeLast(r.last_id); return }
      for (const n of freshNotifications(r.items, last)) {
        void platform.notifications.show({ title: n.title, body: n.body, href: '#/films/requests', conv: '', tag: `entracte-${n.id}`, icon: '' }).catch(() => {})
      }
      if (r.last_id > last) writeLast(r.last_id)
    } catch { /* hors ligne : le prochain relevé rattrapera */ }
  }
}

export const entracte = new EntracteState()
