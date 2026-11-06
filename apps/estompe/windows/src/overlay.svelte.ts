/** Overlay : fenêtre Tauri transparente, au premier plan, traversée par les clics, qui affiche les participants du salon vocal. */
import { app } from '../../frontend/src/lib/store.svelte'
import { voice } from '../../frontend/src/blocks/vocal/voice.svelte'
import { prefs } from '../../frontend/src/lib/prefs.svelte'
import { avatars } from '../../frontend/src/blocks/avatars/avatars.svelte'
import { registerGlobalPtt } from './ptt-global'
import { notify } from '../../frontend/src/lib/ui'
import { overlayItems, overlayPosition, overlaySize, overlayVisible, type OverlayState } from './overlay'

const LABEL = 'overlay'
const POLL_MS = 2000

/** État de l'overlay côté fenêtre principale : masqué par l'utilisateur, jeu au premier plan, écran de ce jeu (pixels physiques). */
export const overlayUi = $state<{ hidden: boolean; game: boolean; monitor: [number, number, number, number] | null }>({ hidden: false, game: false, monitor: null })
export const toggleOverlay = () => { overlayUi.hidden = !overlayUi.hidden }

/** Les avatars sont des URL blob: du webview principal : la fenêtre d'overlay ne les voit pas, on lui passe des data: URI. */
const dataUrls = new Map<string, string>()
export async function dataUrl(blobUrl: string): Promise<string> {
  if (!blobUrl) return ''
  const hit = dataUrls.get(blobUrl)
  if (hit) return hit
  try {
    const blob = avatars.blob(blobUrl)
    if (!blob) return ''
    const url = await new Promise<string>((ok, ko) => { const r = new FileReader(); r.onload = () => ok(r.result as string); r.onerror = ko; r.readAsDataURL(blob) })
    dataUrls.set(blobUrl, url)
    return url
  } catch { return '' }
}

/** Écran du jeu (ou de la fenêtre au premier plan), parmi ceux que connaît Tauri. */
async function foregroundMonitor() {
  if (!overlayUi.monitor) return null
  const { availableMonitors } = await import('@tauri-apps/api/window')
  const [x, y] = overlayUi.monitor
  return (await availableMonitors()).find((m) => m.position.x === x && m.position.y === y) ?? null
}

function snapshot(): OverlayState | null {
  if (!overlayVisible(prefs.overlayWhen, overlayUi.game, overlayUi.hidden) || voice.state !== 'connected' || !voice.channelId) return null
  const serverId = app.channel(voice.channelId)?.server.id
  const items = (app.voice[voice.channelId] ?? []).map((p) => {
    avatars.ensure(p.user_id, serverId)
    const me = p.user_id === app.meId
    const st = voice.states[p.user_id]
    return {
      id: p.user_id, pseudo: p.pseudo, avatar: avatars.src(p.user_id, serverId), speaking: !!voice.speaking[p.user_id],
      muted: me ? voice.muted : !!st?.muted, deaf: me ? voice.deafened : !!st?.deaf,
    }
  })
  return { size: prefs.overlaySize, items: overlayItems(items, prefs.overlayMode) }
}

let queue: Promise<void> = Promise.resolve()
let last: OverlayState | null = null

async function apply(s: OverlayState | null) {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  let w = await WebviewWindow.getByLabel(LABEL)
  if (!s) { await w?.destroy(); return } // destroy : le « close » de la fenêtre principale est détourné en masquage
  const { currentMonitor, LogicalPosition, LogicalSize } = await import('@tauri-apps/api/window')
  const size = overlaySize(s.size, s.items.length)
  const mon = await foregroundMonitor() ?? await currentMonitor()
  const pos = mon ? overlayPosition(prefs.overlayCorner, { x: mon.position.x, y: mon.position.y, w: mon.size.width, h: mon.size.height, sf: mon.scaleFactor }, size) : { x: 16, y: 16 }
  if (!w) {
    w = new WebviewWindow(LABEL, {
      url: 'index.html#overlay', x: pos.x, y: pos.y, width: size.w, height: size.h,
      transparent: true, decorations: false, shadow: false, alwaysOnTop: true, skipTaskbar: true, focus: false, focusable: false, resizable: false,
    })
    await new Promise<void>((ok, ko) => { void w!.once('tauri://created', () => ok()); void w!.once('tauri://error', (e) => ko(new Error(String(e.payload)))) })
    await w.setIgnoreCursorEvents(true)
  } else {
    await w.setSize(new LogicalSize(size.w, size.h))
    await w.setPosition(new LogicalPosition(pos.x, pos.y))
  }
  const items = await Promise.all(s.items.map(async (i) => ({ ...i, avatar: await dataUrl(i.avatar) })))
  const { emit } = await import('@tauri-apps/api/event')
  await emit('overlay-state', { size: s.size, items })
}

/** À appeler une fois au démarrage : suit le salon vocal et les réglages, ouvre et ferme la fenêtre d'overlay. */
export function startOverlay() {
  $effect.root(() => {
    $effect(() => {
      const s = snapshot()
      last = s
      queue = queue.then(() => apply(s)).catch((e) => console.warn('overlay', e))
    })
  })
  // Jeu au premier plan : sondé seulement en salon vocal, mode « Seulement sur les jeux » ou « Toujours » (pour choisir l'écran).
  $effect.root(() => {
    $effect(() => {
      if (prefs.overlayWhen === 'never' || (voice.state !== 'connected' && prefs.msgOverlay === 'off')) return
      let stop = false
      const poll = async () => {
        try {
          const { invoke } = await import('@tauri-apps/api/core')
          const f = await invoke<{ game: boolean; monitor: [number, number, number, number] | null }>('foreground_info')
          if (stop) return
          overlayUi.game = f.game
          if (f.monitor) overlayUi.monitor = f.monitor
        } catch { /* détection indisponible : « Toujours » reste utilisable */ }
      }
      void poll()
      const t = setInterval(poll, POLL_MS)
      return () => { stop = true; clearInterval(t) }
    })
    // Raccourci global pour masquer ou réafficher l'overlay, aussi en jeu.
    $effect(() => {
      if (prefs.overlayWhen === 'never' || voice.state !== 'connected') return
      const key = prefs.overlayKey
      let release: (() => Promise<void>) | null = null
      let stop = false
      registerGlobalPtt(key, (on) => on && toggleOverlay())
        .then((r) => { if (stop) void r(); else release = r })
        .catch((e) => notify.info((e as Error).message))
      return () => { stop = true; void release?.() }
    })
  })
  // La fenêtre d'overlay demande l'état dès qu'elle a chargé (elle peut s'ouvrir après le premier envoi).
  void import('@tauri-apps/api/event').then(({ listen }) => listen('overlay-ready', () => { queue = queue.then(() => apply(last)).catch(() => {}) }))
}
