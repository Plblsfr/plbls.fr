/** Android : notifications du système (plugin Tauri) et push FCM (jeton fourni par MainActivity, rangé côté serveur comme un abonnement push). */
import { api } from '../../frontend/src/lib/api'
import type { NotificationDriver, PushDriver } from '../../frontend/src/lib/platform'
import { nativeBridge } from './bridge'

const FCM_FLAG = 'fcm-enabled'
const fcmOn = () => { try { return localStorage.getItem(FCM_FLAG) === '1' } catch { return false } }
const setFcm = (on: boolean) => { try { on ? localStorage.setItem(FCM_FLAG, '1') : localStorage.removeItem(FCM_FLAG) } catch { /* stockage indisponible */ } }

async function fcmToken(): Promise<string> {
  for (let i = 0; i < 20; i++) { // FirebaseMessaging répond de façon asynchrone après le démarrage
    const t = nativeBridge()?.fcmToken()
    if (t) return t
    await new Promise((r) => setTimeout(r, 500))
  }
  return ''
}

/** Android 13+ : permission à demander à l'exécution. */
async function ask(): Promise<boolean> {
  const { isPermissionGranted, requestPermission } = await import('@tauri-apps/plugin-notification')
  return (await isPermissionGranted()) || (await requestPermission()) === 'granted'
}

const push: PushDriver = {
  supported: () => true,
  probeServer: false,
  enabled: async () => fcmOn(),
  async enable() {
    if (!(await ask())) return false
    const token = await fcmToken()
    if (!token) throw new Error('FCM indisponible')
    await api.fcmSubscribe(token)
    setFcm(true)
    return true
  },
  async disable() {
    const token = nativeBridge()?.fcmToken()
    if (token) await api.fcmUnsubscribe(token).catch(() => {})
    setFcm(false)
  },
  /** Au démarrage : le jeton FCM peut changer, on le réenregistre si le push est activé. */
  async refresh() {
    if (!fcmOn()) return
    const token = await fcmToken()
    if (token) await api.fcmSubscribe(token).catch(() => {})
  },
}

export const notifications: NotificationDriver = {
  push,
  testable: false,
  supported: () => true,
  needsGesture: () => false,
  ask,
  async show(n) {
    const { sendNotification } = await import('@tauri-apps/plugin-notification')
    if (await ask()) sendNotification({ title: n.title, body: n.body })
  },
}
