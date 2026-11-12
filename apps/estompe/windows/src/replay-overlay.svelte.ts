import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'
import { invoke } from '../../crates/core/ts/tauri'
import { overlayPosition } from './overlay'
import { coinOverlay, etatOverlay, tailleOverlay, type EtatOverlay } from './replay'
import { replay } from './replay.svelte'

const LABEL = 'replay-overlay'
let file: Promise<void> = Promise.resolve()
let dernier: EtatOverlay | null = null

async function ecranDuJeu() {
  const { availableMonitors, currentMonitor } = await import('@tauri-apps/api/window')
  const ecran = (await invoke<{ monitor: [number, number, number, number] | null }>('foreground_info').catch(() => null))?.monitor
  return (ecran && (await availableMonitors()).find((m) => m.position.x === ecran[0] && m.position.y === ecran[1])) || await currentMonitor()
}

async function appliquer(s: EtatOverlay | null) {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  let w = await WebviewWindow.getByLabel(LABEL)
  if (!s) { await w?.destroy(); return }
  const { LogicalPosition, LogicalSize } = await import('@tauri-apps/api/window')
  const taille = tailleOverlay(s)
  const mon = await ecranDuJeu()
  const pos = mon ? overlayPosition(coinOverlay(prefs.overlayCorner), { x: mon.position.x, y: mon.position.y, w: mon.size.width, h: mon.size.height, sf: mon.scaleFactor }, taille) : { x: 16, y: 16 }
  if (!w) {
    w = new WebviewWindow(LABEL, {
      url: 'index.html#replay-overlay', x: pos.x, y: pos.y, width: taille.w, height: taille.h,
      transparent: true, decorations: false, shadow: false, alwaysOnTop: true, skipTaskbar: true, focus: false, focusable: false, resizable: false,
    })
    await new Promise<void>((ok, ko) => { void w!.once('tauri://created', () => ok()); void w!.once('tauri://error', (e) => ko(new Error(String(e.payload)))) })
    await w.setIgnoreCursorEvents(true)
  } else {
    await w.setSize(new LogicalSize(taille.w, taille.h))
    await w.setPosition(new LogicalPosition(pos.x, pos.y))
  }
  const { emit } = await import('@tauri-apps/api/event')
  await emit('replay-overlay-state', s)
}

export function startReplayOverlay() {
  $effect.root(() => {
    $effect(() => {
      const s = etatOverlay(replay.etat, replay.reglages.actif, $state.snapshot(replay.messages))
      void prefs.overlayCorner
      dernier = s
      file = file.then(() => appliquer(s)).catch((e) => console.warn('replay-overlay', e))
    })
  })
  void import('@tauri-apps/api/event').then(({ listen }) => listen('replay-overlay-ready', () => { file = file.then(() => appliquer(dernier)).catch(() => {}) }))
}
