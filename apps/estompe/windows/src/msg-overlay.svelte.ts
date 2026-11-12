/** Overlay des messages : fenêtre Tauri transparente qui montre les MP et mentions reçus en jeu, avec réponse rapide au clavier. */
import { app } from '../../../../web/estompe/src/lib/store.svelte'
import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'
import { avatars } from '../../../../web/estompe/src/blocks/avatars/avatars.svelte'
import { router } from '../../../../web/estompe/src/lib/route.svelte'
import { registerGlobalPtt } from './ptt-global'
import { tapNotifications } from '../../../../web/estompe/src/blocks/notifications/notify'
import { notify } from '../../../../web/estompe/src/lib/ui'
import { dataUrl, overlayUi } from './overlay.svelte'
import { msgOverlaySize, msgToastView, msgToastWanted, overlayPosition, overlayVisible, pushMsgToast, sendMsgReply, type MsgOverlayState, type MsgToast } from './overlay'

const LABEL = 'msg-overlay'

const live = $state<{ toasts: MsgToast[]; replying: boolean }>({ toasts: [], replying: false })
const timers = new Map<string, ReturnType<typeof setTimeout>>()
let queue: Promise<void> = Promise.resolve()
let last: MsgOverlayState | null = null

export function expirer(timers: Map<string, ReturnType<typeof setTimeout>>, id: string, retirer: () => boolean) {
  clearTimeout(timers.get(id))
  timers.set(id, setTimeout(() => {
    timers.delete(id)
    if (!retirer()) expirer(timers, id, retirer)
  }, Math.max(2, prefs.msgSeconds) * 1000))
}

function expire(id: string) {
  expirer(timers, id, () => {
    if (live.replying) return false // la saisie en cours garde les notifications
    live.toasts = live.toasts.filter((t) => t.id !== id)
    return true
  })
}

function onNotification(n: Parameters<Parameters<typeof tapNotifications>[0]>[0]) {
  if (!n.msg || !n.conv || !msgToastWanted(n.kind, true, prefs.msgOverlay)) return
  const author = app.convs[n.conv]?.messages.find((m) => m.id === n.msg)?.author.id ?? ''
  if (author) avatars.ensure(author)
  live.toasts = pushMsgToast(live.toasts, { id: n.msg, conv: n.conv, href: n.url.replace(/^\//, ''), title: n.title, body: n.body, avatar: author ? avatars.src(author) : '', author })
  expire(n.msg)
}

function snapshot(): MsgOverlayState | null {
  if (!live.toasts.length || !overlayVisible(prefs.overlayWhen, overlayUi.game, overlayUi.hidden)) return null
  return { toasts: live.toasts.map((t) => msgToastView(t, prefs.msgHide)), replying: live.replying }
}

async function apply(s: MsgOverlayState | null) {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  let w = await WebviewWindow.getByLabel(LABEL)
  if (!s) { await w?.destroy(); return }
  const { availableMonitors, currentMonitor, LogicalPosition, LogicalSize } = await import('@tauri-apps/api/window')
  const size = msgOverlaySize(s.toasts.length, s.replying)
  const [x, y] = overlayUi.monitor ?? []
  const mon = (overlayUi.monitor && (await availableMonitors()).find((m) => m.position.x === x && m.position.y === y)) || await currentMonitor()
  const pos = mon ? overlayPosition(prefs.overlayCorner, { x: mon.position.x, y: mon.position.y, w: mon.size.width, h: mon.size.height, sf: mon.scaleFactor }, size) : { x: 16, y: 16 }
  if (!w) {
    w = new WebviewWindow(LABEL, {
      url: 'index.html#msg-overlay', x: pos.x, y: pos.y, width: size.w, height: size.h,
      transparent: true, decorations: false, shadow: false, alwaysOnTop: true, skipTaskbar: true, focus: false, focusable: false, resizable: false,
    })
    await new Promise<void>((ok, ko) => { void w!.once('tauri://created', () => ok()); void w!.once('tauri://error', (e) => ko(new Error(String(e.payload)))) })
    await w.setIgnoreCursorEvents(true)
  } else {
    await w.setSize(new LogicalSize(size.w, size.h))
    await w.setPosition(new LogicalPosition(pos.x, pos.y))
  }
  // Le champ de réponse rend le focus à la fenêtre le temps de la saisie, puis la fenêtre redevient intouchable.
  await w.setFocusable(s.replying)
  await w.setIgnoreCursorEvents(!s.replying)
  if (s.replying) await w.setFocus()
  const toasts = await Promise.all(s.toasts.map(async (t) => ({ ...t, avatar: await dataUrl(t.avatar) })))
  const { emit } = await import('@tauri-apps/api/event')
  await emit('msg-overlay-state', { toasts, replying: s.replying })
}

async function openLast() {
  const t = live.toasts[live.toasts.length - 1]
  if (!t) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  const win = getCurrentWindow()
  await win.show()
  await win.unminimize()
  await win.setFocus()
  router.go(t.href)
  app.readConv(t.conv)
  live.toasts = live.toasts.filter((x) => x.conv !== t.conv)
}

/** À appeler une fois au démarrage (app Windows) : suit les notifications reçues et ouvre ou ferme la fenêtre d'overlay des messages. */
export function startMsgOverlay() {
  tapNotifications(onNotification)
  $effect.root(() => {
    $effect(() => {
      const s = snapshot()
      last = s
      queue = queue.then(() => apply(s)).catch((e) => console.warn('msg-overlay', e))
    })
    // Raccourcis globaux, actifs tant que des notifications sont affichées (ils ne volent pas la touche le reste du temps).
    for (const [key, run] of [[() => prefs.msgReplyKey, () => { if (live.toasts.length) live.replying = true }], [() => prefs.msgOpenKey, () => void openLast()]] as const) {
      $effect(() => {
        const code = key()
        if (prefs.msgOverlay === 'off' || !live.toasts.length) return
        let release: (() => Promise<void>) | null = null
        let stop = false
        registerGlobalPtt(code, (on) => on && run())
          .then((r) => { if (stop) void r(); else release = r })
          .catch((e) => notify.info((e as Error).message))
        return () => { stop = true; void release?.() }
      })
    }
  })
  void import('@tauri-apps/api/event').then(({ listen }) => {
    void listen('msg-overlay-ready', () => { queue = queue.then(() => apply(last)).catch(() => {}) })
    void listen('msg-overlay-cancel', () => { live.replying = false })
    void listen<string>('msg-overlay-send', async (e) => {
      try {
        live.toasts = await sendMsgReply(live.toasts, e.payload, (c, t) => app.send(c, t), (c) => app.readConv(c))
      } catch (err) { notify.info((err as Error).message) }
      live.replying = false
    })
  })
}
