/** Windows : toasts natifs (Rust), boutons « Répondre » et « Marquer comme lu », pastille sur l'icône de la barre des tâches. */
import type { NotificationDriver } from '../../frontend/src/lib/platform'
import { router } from '../../frontend/src/lib/route.svelte'
import { invoke } from '../../crates/core/ts/tauri'

/** Action choisie sur un toast : « read » marque la conversation lue, sinon ouvre l'endroit de la notification. */
export function onToastAction(d: { action: string; href: string; conv: string }, markRead: (conv: string) => void) {
  if (d.action === 'read') return d.conv && markRead(d.conv)
  if (d.href) router.go(d.href)
}

async function badge(n: number) {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    if (!n) return await getCurrentWindow().setOverlayIcon()
    const { Image } = await import('@tauri-apps/api/image')
    const c = document.createElement('canvas')
    c.width = c.height = 32
    const g = c.getContext('2d')!
    g.fillStyle = '#d9342b'
    g.beginPath()
    g.arc(16, 16, 16, 0, Math.PI * 2)
    g.fill()
    g.fillStyle = '#fff'
    g.font = 'bold 20px sans-serif'
    g.textAlign = 'center'
    g.textBaseline = 'middle'
    g.fillText(n > 9 ? '9+' : String(n), 16, 17)
    const png = new Uint8Array(await (await new Promise<Blob>((r) => c.toBlob((b) => r(b!), 'image/png'))).arrayBuffer())
    await getCurrentWindow().setOverlayIcon(await Image.fromBytes(png))
  } catch (e) { console.warn('badge de la barre des tâches impossible', e) }
}

export const notifications: NotificationDriver = {
  push: null,
  testable: false,
  supported: () => true,
  needsGesture: () => false,
  ask: async () => true,
  show: (n) => invoke('toast_show', { title: n.title, body: n.body, href: n.href, conv: n.conv }),
  setBadge: badge,
  listenActions(markRead) {
    void import('@tauri-apps/api/event').then(({ listen }) => listen<{ action: string; href: string; conv: string }>('toast-action', (e) => onToastAction(e.payload, markRead)))
  },
}
