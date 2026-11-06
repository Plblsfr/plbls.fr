import { api } from '../../../../web/estompe/src/lib/api'
import { app } from '../../../../web/estompe/src/lib/store.svelte'
import { notify } from '../../../../web/estompe/src/lib/ui'
import { debug } from '../../../../web/estompe/src/lib/debuglog.svelte'
import { voice } from '../../../../web/estompe/src/blocks/vocal/voice.svelte'
import { watchView } from '../../../../web/estompe/src/blocks/regarder-ensemble/watch.svelte'
import { music } from '../../../../web/estompe/src/blocks/musique/music.svelte'
import { coverUrl } from '../../../../web/estompe/src/blocks/musique/music'
import { solo } from '../../../../web/estompe/src/blocks/videos/solo.svelte'
import type { SessionKind } from '../../../../web/estompe/src/lib/types'
import { nativeBridge } from './bridge'
import { bgKind, bgPayload, bgRun, stallCheck, type BgKind, type Sample } from './bgaudio'

const KEY = 'estompe.bgAudio'

function read() {
  try { return localStorage.getItem(KEY) === '1' } catch { return false }
}

export const bgAudio = $state({ on: read() })

export function setBgAudio(on: boolean) {
  bgAudio.on = on
  try { localStorage.setItem(KEY, on ? '1' : '0') } catch { /* ignoré */ }
}

function session(kind: 'watch' | 'music') {
  const cid = voice.channelId && voice.state !== 'idle' ? voice.channelId : ''
  const k: SessionKind = kind
  return { cid, k, state: cid ? app[k][cid]?.state : undefined, player: kind === 'music' ? music.player : watchView.player }
}

function players() {
  return { watch: !!session('watch').state?.video, music: !!session('music').state?.video, solo: !!solo.current }
}

function info(kind: BgKind) {
  if (kind === 'solo') {
    const t = solo.current
    return { title: t?.title ?? '', artist: t?.artist ?? '', cover: t ? coverUrl(t) : '', playing: solo.playing, position: solo.position }
  }
  const s = session(kind)
  const m = s.state?.meta
  return { title: m?.title ?? (kind === 'music' ? 'Music2gether' : 'Regarder ensemble'), artist: m?.artist ?? '', cover: m ? coverUrl(m) : '', playing: !!s.state?.playing, position: s.player?.position ?? 0 }
}

function actions(kind: BgKind) {
  if (kind === 'solo') return { toggle: () => solo.toggle(), next: () => solo.next() }
  const s = session(kind)
  const fail = (e: unknown) => notify.error(e, 'Action impossible')
  return {
    toggle: () => void api.watchAction(s.cid, s.state?.playing ? 'pause' : 'play', { position: s.player?.position ?? s.state?.pos ?? 0 }, s.k).catch(fail),
    next: () => void api.watchAction(s.cid, 'ended', { rev: s.state!.rev }, s.k).catch(fail),
  }
}

export function setupBgAudio() {
  const bridge = nativeBridge()
  if (!bridge?.bgAudioUpdate) return
  let stalled = $state(false)
  let sample: Sample | null = null
  $effect.root(() => {
    $effect(() => {
      const kind = bgKind(players())
      const i = kind ? info(kind) : null
      const payload = i ? bgPayload(bgAudio.on, kind, { title: i.title, artist: i.artist, cover: i.cover, playing: i.playing, stalled }) : null
      if (payload) bridge.bgAudioUpdate?.(JSON.stringify(payload))
      else bridge.bgAudioStop?.()
    })
  })
  setInterval(() => {
    const kind = bgKind(players())
    const i = kind && bgAudio.on ? info(kind) : null
    const r = stallCheck(sample, i?.position ?? 0, Date.now(), !!i?.playing && document.hidden)
    sample = r.sample
    if (r.stalled !== stalled) {
      stalled = r.stalled
      debug.log(r.stalled ? 'warn' : 'info', 'audio', r.stalled ? 'lecture en arrière-plan interrompue (lecteur mis en pause par YouTube ?)' : 'lecture en arrière-plan reprise')
    }
  }, 5000)
  ;(window as unknown as { __estompeBg: (a: string) => void }).__estompeBg = (a) => {
    const kind = bgKind(players())
    if (kind) bgRun(a, actions(kind))
  }
}
