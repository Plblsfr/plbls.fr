import { api } from '../../frontend/src/lib/api'
import { app } from '../../frontend/src/lib/store.svelte'
import { prefs } from '../../frontend/src/lib/prefs.svelte'
import { notify } from '../../frontend/src/lib/ui'
import { voice } from '../../frontend/src/blocks/vocal/voice.svelte'
import { watchView, type WatchPlayer } from '../../frontend/src/blocks/regarder-ensemble/watch.svelte'
import { music } from '../../frontend/src/blocks/musique/music.svelte'
import { solo } from '../../frontend/src/blocks/videos/solo.svelte'
import { nativeBridge } from './bridge'
import type { SessionKind } from '../../frontend/src/lib/types'
import { bgAudio } from './bgaudio.svelte'
import { pipKind, pipRun, pipState, type PipEvent, type PipKind } from './pip'

function session(kind: PipKind) {
  const cid = voice.channelId && voice.state !== 'idle' ? voice.channelId : ''
  const k: SessionKind = kind === 'music' ? 'music' : 'watch'
  return { cid, k, state: cid ? app[k][cid]?.state : undefined, player: (kind === 'music' ? music.player : watchView.player) as WatchPlayer | null }
}

function players() {
  return { watch: !!session('watch').state?.video && !!watchView.player, music: !!session('music').state?.video && !!music.player, solo: !!solo.current }
}

function playing(kind: PipKind) { return kind === 'solo' ? solo.playing : !!session(kind).state?.playing }

function actionsOf(kind: PipKind) {
  const s = kind === 'solo' ? null : session(kind)
  const command = (action: 'play' | 'pause') => s?.player?.command({ action, position: s.player.position })
  return {
    toggle: () => (s ? command(s.state?.playing ? 'pause' : 'play') : solo.toggle()),
    next: () => (s ? void api.watchAction(s.cid, 'ended', { rev: s.state!.rev }, s.k).catch((e) => notify.error(e, 'Action impossible')) : solo.next()),
    close: () => (s ? void voice.leave() : solo.stop()),
    pause: () => { if (s) { if (s.player?.canDrive) command('pause') } else solo.playing = false },
  }
}

/** Image dans l'image : envoie l'état du lecteur au natif et applique les commandes de la fenêtre flottante. */
export function setupPip() {
  const bridge = nativeBridge()
  if (!bridge?.pipState) return
  let notice = false
  $effect.root(() => {
    $effect(() => {
      const kind = pipKind(players())
      bridge.pipState?.(JSON.stringify(pipState(prefs.floatingPlayer && !bgAudio.on, kind, kind ? playing(kind) : false)))
    })
  })
  ;(window as unknown as { estompePip: (e: PipEvent) => void }).estompePip = (evt) => {
    const kind = pipKind(players())
    if (evt === 'enter' && kind) document.documentElement.dataset.pip = kind
    else if (evt === 'leave') delete document.documentElement.dataset.pip
    else if (kind && pipRun(evt, actionsOf(kind))) notice = true
  }
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible' && notice) { notice = false; notify.info("Le lecteur flottant n'est pas disponible sur cet appareil : la lecture a été mise en pause.") }
  })
}
