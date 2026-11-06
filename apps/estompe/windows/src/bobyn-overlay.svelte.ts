/** Pastille Bobyn : petite fenêtre transparente (comme l'overlay des participants) qui indique l'état de l'enregistrement sur l'écran du jeu. */
import type { BobynStatus } from './bobyn'
import { bobynState } from './bobyn'
import { overlayUi } from './overlay.svelte'
import { BOBYN_OVERLAY_SIZE, bobynOverlayState, overlayPosition, type BobynOverlayState } from './overlay'
import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'
import { offerSend, quickSend } from './replaySend.svelte'

const LABEL = 'bobyn-overlay'
const POLL_MS = 2000
const SAVED_MS = 4000
const MARKED_MS = 1500

const live = $state<{ status: BobynStatus | null; saved: string | null; marked: boolean }>({ status: null, saved: null, marked: false })
let queue: Promise<void> = Promise.resolve()
let last: BobynOverlayState | null = null

async function apply(s: BobynOverlayState | null) {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  let w = await WebviewWindow.getByLabel(LABEL)
  if (!s) { await w?.destroy(); return }
  const { availableMonitors, currentMonitor, LogicalPosition } = await import('@tauri-apps/api/window')
  const [x, y] = overlayUi.monitor ?? []
  const mon = (overlayUi.monitor && (await availableMonitors()).find((m) => m.position.x === x && m.position.y === y)) || await currentMonitor()
  const corner = prefs.bobynOverlay === 'off' ? 'tr' : prefs.bobynOverlay
  const pos = mon ? overlayPosition(corner, { x: mon.position.x, y: mon.position.y, w: mon.size.width, h: mon.size.height, sf: mon.scaleFactor }, BOBYN_OVERLAY_SIZE) : { x: 16, y: 16 }
  if (!w) {
    w = new WebviewWindow(LABEL, {
      url: 'index.html#bobyn-overlay', x: pos.x, y: pos.y, width: BOBYN_OVERLAY_SIZE.w, height: BOBYN_OVERLAY_SIZE.h,
      transparent: true, decorations: false, shadow: false, alwaysOnTop: true, skipTaskbar: true, focus: false, focusable: false, resizable: false,
    })
    await new Promise<void>((ok, ko) => { void w!.once('tauri://created', () => ok()); void w!.once('tauri://error', (e) => ko(new Error(String(e.payload)))) })
    await w.setIgnoreCursorEvents(true)
  } else {
    await w.setPosition(new LogicalPosition(pos.x, pos.y))
  }
  const { emit } = await import('@tauri-apps/api/event')
  await emit('bobyn-overlay-state', s)
}

/** À appeler une fois au démarrage (app Windows) : suit l'état de Bobyn et ouvre ou ferme la pastille. */
export function startBobynOverlay() {
  void (async () => {
    const { listen } = await import('@tauri-apps/api/event')
    live.status = (await bobynState().catch(() => null))?.status ?? null
    await listen<BobynStatus>('bobyn-status', (e) => (live.status = e.payload))
    let timer: ReturnType<typeof setTimeout> | undefined
    await listen<{ action: string; conv: string }>('toast-action', (e) => {
      if (e.payload.action === 'send' && e.payload.conv.startsWith('replay:')) void quickSend(e.payload.conv.slice('replay:'.length))
    })
    await listen<string>('bobyn-saved', (e) => {
      void bobynState().then((s) => offerSend(e.payload, s.config.notify)).catch(() => {})
      live.saved = e.payload
      clearTimeout(timer)
      timer = setTimeout(() => (live.saved = null), SAVED_MS)
    })
    let markTimer: ReturnType<typeof setTimeout> | undefined
    await listen('bobyn-marker', () => {
      live.marked = true
      clearTimeout(markTimer)
      markTimer = setTimeout(() => (live.marked = false), MARKED_MS)
    })
    await listen('bobyn-overlay-ready', () => { queue = queue.then(() => apply(last)).catch(() => {}) })
  })()
  $effect.root(() => {
    $effect(() => {
      const s = bobynOverlayState(prefs.bobynOverlay, live.status, overlayUi.game, overlayUi.hidden, live.saved, live.marked)
      last = s
      queue = queue.then(() => apply(s)).catch((e) => console.warn('bobyn-overlay', e))
    })
    // Jeu au premier plan et écran à utiliser : sondé seulement quand Bobyn enregistre.
    $effect(() => {
      if (prefs.bobynOverlay === 'off' || !live.status || live.status.state === 'idle') return
      let stop = false
      const poll = async () => {
        try {
          const { invoke } = await import('@tauri-apps/api/core')
          const f = await invoke<{ game: boolean; monitor: [number, number, number, number] | null }>('foreground_info')
          if (stop) return
          overlayUi.game = f.game
          if (f.monitor) overlayUi.monitor = f.monitor
        } catch { /* détection indisponible */ }
      }
      void poll()
      const t = setInterval(poll, POLL_MS)
      return () => { stop = true; clearInterval(t) }
    })
  })
}
