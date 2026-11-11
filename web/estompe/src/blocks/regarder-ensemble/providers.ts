import { filmsApi, ApiError } from '../../lib/api'
import { debug } from '../../lib/debuglog.svelte'
import { absolute, needsReopen, saveDue, savedQuality, saveQuality, support, type FilmPlayback, type FilmTracks, type Quality } from '../../sections/entracte/films'
import { playerError } from './watch'

import type Hls from 'hls.js'

/** État d'un lecteur, commun à toutes les sources. */
export type PState = 'unstarted' | 'playing' | 'paused' | 'buffering' | 'ended' | 'cued'

export interface Handlers {
  onState(s: PState): void
  onError(msg: string): void
  /** Le navigateur ou la webview a refusé la lecture automatique. */
  onBlocked?(): void
  onTracks?(t: FilmTracks): void
}

/**
 * Interface commune des sources de vidéo : « Regarder ensemble » n'en connaît que ces méthodes. Ajouter une source =
 * écrire une fabrique ici et reconnaître ses liens côté serveur (`api/videos.go`).
 */
export interface Adapter {
  /** `full` : lecture, pause, position et vitesse sont synchronisées ; `start` : seul le lancement de la vidéo l'est (direct, clip). */
  readonly sync: 'full' | 'start'
  /** La vitesse de lecture peut être changée (sinon un petit écart n'est pas rattrapé). */
  readonly canRate: boolean
  /** Volume propre à la personne, de 0 à 100 (absent : le lecteur n'en a pas). */
  setVolume?(v: number): void
  setAudio?(n: number): void
  setSubtitle?(n: number | null): void
  /** Palier de qualité (Entracte) : rouvre le flux à la même position. */
  setQuality?(q: Quality): void
  load(ref: string, start: number, play: boolean): void
  play(): void
  pause(): void
  seek(s: number): void
  time(): number
  /** Durée de la vidéo (s), 0 si inconnue (direct). */
  duration?(): number
  rate(): number
  setRate(r: number): void
  state(): PState
  destroy(): void
}
export type Factory = (el: HTMLElement, h: Handlers) => Promise<Adapter>

export const PROVIDER_LABEL: Record<string, string> = {
  youtube: 'YouTube', ytmusic: 'YouTube Music', twitch_vod: 'Twitch', twitch_clip: 'Twitch (clip)', twitch_live: 'Twitch (direct)', vimeo: 'Vimeo', dailymotion: 'Dailymotion', file: 'Fichier', page: 'Fichier', simple: 'Lien', film: 'Films et séries',
}

/** Message reçu d'un lecteur intégré : objet, ou JSON en texte ; autre chose : null. */
function parseMessage(data: unknown): Record<string, unknown> | null {
  let d = data
  if (typeof d === 'string') { try { d = JSON.parse(d) } catch { return null } }
  return objet(d)
}
const objet = (x: unknown): Record<string, unknown> | null => x && typeof x === 'object' && !Array.isArray(x) ? (x as Record<string, unknown>) : null
const num = (x: unknown): x is number => typeof x === 'number' && Number.isFinite(x)

/** iframe d'un lecteur : ses messages ne sont acceptés que de son origine et de sa fenêtre. */
function playerFrame(el: HTMLElement, origin: string, onMessage: (data: unknown) => void) {
  const f = document.createElement('iframe')
  f.allowFullscreen = true
  f.allow = 'autoplay; encrypted-media; picture-in-picture; fullscreen'
  f.referrerPolicy = 'strict-origin-when-cross-origin'
  f.style.cssText = 'width:100%;height:100%;border:0'
  el.appendChild(f)
  const listener = (e: MessageEvent) => { if (e.origin === origin && e.source === f.contentWindow) onMessage(e.data) }
  addEventListener('message', listener)
  return { f, post: (m: unknown) => f.contentWindow?.postMessage(JSON.stringify(m), origin), remove: () => { removeEventListener('message', listener); f.remove() } }
}

// --- YouTube : lecteur intégré piloté par postMessage (protocole de l'IFrame API, sans son script) ---

const YT_ORIGIN = 'https://www.youtube-nocookie.com'
const YT_STATE: Record<number, PState> = { [-1]: 'unstarted', 0: 'ended', 1: 'playing', 2: 'paused', 3: 'buffering', 5: 'cued' }

export interface YouTubeInfo { ready?: true; state?: PState; time?: number; duration?: number; rate?: number; error?: number; blocked?: true }

/** Évènement du lecteur YouTube (`onReady`, `onStateChange`, `infoDelivery`…) ramené à ce qu'Estompe en lit. */
export function readYouTube(data: unknown): YouTubeInfo | null {
  const m = parseMessage(data)
  if (!m) return null
  switch (m.event) {
    case 'onReady': return { ready: true }
    case 'onStateChange': return num(m.info) ? { state: YT_STATE[m.info] ?? 'unstarted' } : null
    case 'onError': return { error: num(m.info) ? m.info : 0 }
    case 'onAutoplayBlocked': return { blocked: true }
    case 'onPlaybackRateChange': return num(m.info) ? { rate: m.info } : null
    case 'initialDelivery': case 'infoDelivery': {
      const i = objet(m.info)
      if (!i) return null
      const r: YouTubeInfo = {}
      if (num(i.currentTime)) r.time = i.currentTime
      if (num(i.duration)) r.duration = i.duration
      if (num(i.playbackRate)) r.rate = i.playbackRate
      if (num(i.playerState)) r.state = YT_STATE[i.playerState] ?? 'unstarted'
      return r
    }
  }
  return null
}

const youtube: Factory = async (el, h) => {
  const widget = Math.floor(Math.random() * 1e9)
  let st: PState = 'unstarted'
  let t = 0
  let d = 0
  let r = 1
  let ready = false
  let waiting: { func: string; args: unknown[] }[] = []
  const set = (s: PState) => { if (s !== st) { st = s; h.onState(s) } }
  const frame = playerFrame(el, YT_ORIGIN, (data) => {
    const i = readYouTube(data)
    if (!i) return
    if (i.ready && !ready) { ready = true; clearInterval(hello); for (const c of waiting) send(c.func, c.args); waiting = [] }
    if (i.time !== undefined) t = i.time
    if (i.duration !== undefined) d = i.duration
    if (i.rate !== undefined) r = i.rate
    if (i.state) set(i.state)
    if (i.error !== undefined) h.onError(playerError(i.error))
    if (i.blocked) h.onBlocked?.()
  })
  const post = (m: Record<string, unknown>) => frame.post({ ...m, id: widget, channel: 'widget' })
  const listen = () => post({ event: 'listening' })
  let hello: ReturnType<typeof setInterval> | undefined
  frame.f.addEventListener('load', () => { if (!ready) { listen(); clearInterval(hello); hello = setInterval(listen, 250) } })
  const send = (func: string, args: unknown[] = []) => post({ event: 'command', func, args })
  const call = (func: string, args: unknown[] = []) => (ready ? send(func, args) : waiting.push({ func, args }))
  return {
    sync: 'full', canRate: true,
    load: (ref, start, play) => {
      t = start
      if (!frame.f.src) {
        const q = new URLSearchParams({ enablejsapi: '1', origin: location.origin, widgetid: String(widget), playsinline: '1', rel: '0', controls: '0', disablekb: '1', fs: '0', start: String(Math.floor(start)), autoplay: play ? '1' : '0' })
        frame.f.src = `${YT_ORIGIN}/embed/${encodeURIComponent(ref)}?${q}`
        return
      }
      call(play ? 'loadVideoById' : 'cueVideoById', [{ videoId: ref, startSeconds: start }])
    },
    play: () => call('playVideo'), pause: () => call('pauseVideo'), seek: (s) => { t = s; call('seekTo', [s, true]) },
    time: () => t, duration: () => d, rate: () => r, setRate: (x) => call('setPlaybackRate', [x]), setVolume: (v) => call('setVolume', [v]),
    state: () => st,
    destroy: () => { clearInterval(hello); frame.remove() },
  }
}

// --- Fichiers directs (MP4, WebM, flux HLS) : lecteur HTML5 d'Estompe ---

const isHls = (u: string) => /\.m3u8(\?|#|$)/i.test(u)

function htmlVideo(el: HTMLElement, h: Handlers, crossOrigin = false): { adapter: Adapter; video: HTMLVideoElement } {
  const v = document.createElement('video')
  v.playsInline = true
  if (crossOrigin) v.crossOrigin = 'anonymous'
  v.style.cssText = 'width:100%;height:100%;background:#000'
  el.appendChild(v)
  let hls: Hls | null = null
  let buffering = false
  const state = (): PState => {
    if (v.ended) return 'ended'
    if (!v.paused) return buffering || v.readyState < 3 ? 'buffering' : 'playing'
    return v.readyState >= 1 ? 'paused' : 'unstarted'
  }
  const emit = () => h.onState(state())
  v.addEventListener('waiting', () => { buffering = true; emit() })
  for (const ev of ['playing', 'canplay', 'pause', 'ended', 'seeked']) v.addEventListener(ev, () => { if (ev !== 'pause' || !v.seeking) { buffering = false; emit() } })
  v.addEventListener('error', () => {
    debug.log('warn', 'lecteur', `erreur vidéo ${v.error?.code ?? '?'} : ${v.error?.message || 'sans détail'}`)
    h.onError('Cette vidéo ne peut pas être lue (format ou accès refusé par le site qui l\'héberge).')
  })
  const detach = () => { hls?.destroy(); hls = null }
  const adapter: Adapter = {
    sync: 'full', canRate: true, setVolume: (x) => { v.volume = x / 100; v.muted = x === 0 },
    load: (ref, start, play) => {
      detach()
      buffering = false
      const go = () => { if (start > 0) v.currentTime = start; if (play) void v.play().catch((e: unknown) => {
        if (e instanceof DOMException && (e.name === 'AbortError' || e.name === 'NotSupportedError')) return
        debug.log('warn', 'lecteur', `lecture refusée : ${e instanceof DOMException ? e.name : String(e)}`)
        h.onState('paused')
      }) }
      const native = () => {
        v.src = ref
        v.addEventListener('loadedmetadata', go, { once: true })
        v.load()
      }
      if (isHls(ref)) {
        void import('hls.js').then(({ default: Hls }) => {
          if (!Hls.isSupported()) {
            if (v.canPlayType('application/vnd.apple.mpegurl')) return native()
            return h.onError("Ce flux HLS n'est pas pris en charge par cet appareil.")
          }
          const l = new Hls({ enableWorker: false })
          hls = l
          l.on(Hls.Events.ERROR, (_, d) => {
            if (!d.fatal) return
            debug.log('warn', 'lecteur', `HLS ${d.type} : ${d.details}${d.response ? ` (HTTP ${d.response.code})` : ''}`)
            h.onError('Le flux vidéo ne peut pas être lu (réseau ou accès refusé).')
          })
          l.on(Hls.Events.MANIFEST_PARSED, go)
          l.loadSource(ref)
          l.attachMedia(v)
        }).catch(() => v.canPlayType('application/vnd.apple.mpegurl') ? native() : h.onError("Le lecteur HLS n'a pas pu être chargé."))
        return
      }
      native()
    },
    play: () => void v.play().catch(() => {}), pause: () => v.pause(), seek: (s) => { v.currentTime = s },
    time: () => v.currentTime, duration: () => (Number.isFinite(v.duration) ? v.duration : 0), rate: () => v.playbackRate, setRate: (r) => { v.playbackRate = r },
    state,
    destroy: () => { detach(); v.pause(); v.removeAttribute('src'); v.load(); v.remove() },
  }
  return { adapter, video: v }
}

const file: Factory = async (el, h) => htmlVideo(el, h).adapter


const ATTENTE_DIRECT_MS = 10_000

const film: Factory = async (el, h) => {
  let ref = ''
  let audio = 0
  let sub: number | null = null
  let quality: Quality = 'original'
  let stream: FilmPlayback | null = null
  let offset = 0
  let forceHls = false
  let retried = false
  let gone = false
  let opening = 0
  let savedAt = 0
  let position = 0
  let guet: ReturnType<typeof setTimeout> | undefined
  const store = () => { try { return localStorage } catch { return null } }

  const close = () => { if (stream) void filmsApi.close(stream.stream_id).catch(() => {}); stream = null }
  const total = () => stream?.duration || (Number.isFinite(video.duration) ? video.duration + offset : 0)
  const save = () => {
    const duration = total()
    if (!ref || position <= 0 || !Number.isFinite(duration) || duration <= 0) return
    savedAt = Date.now()
    void filmsApi.progress(ref, position, duration).catch(() => {})
  }
  const showSubtitle = () => { for (let i = 0; i < video.textTracks.length; i++) video.textTracks[i].mode = i === sub ? 'showing' : 'disabled' }
  const open = async (start: number, play: boolean) => {
    const turn = ++opening
    clearTimeout(guet)
    close()
    const ask = (q: Quality) => filmsApi.playback(ref, {
      direct: !forceHls,
      audio, quality: q, start, support: support((t) => video.canPlayType(t)),
    })
    try {
      let p: FilmPlayback
      try { p = await ask(quality) } catch (e) {
        if (!(e instanceof ApiError && e.status === 400 && quality !== 'original')) throw e
        quality = 'original'
        p = await ask(quality)
      }
      if (gone || turn !== opening) { void filmsApi.close(p.stream_id).catch(() => {}); return }
      stream = p
      debug.log('info', 'lecteur', `flux ${p.mode}, qualité ${p.quality ?? quality}`)
      if (p.mode === 'direct' && play) guet = setTimeout(() => { if (stream === p && video.readyState < 2) repli(`aucune image après ${ATTENTE_DIRECT_MS / 1000} s en lecture directe`) }, ATTENTE_DIRECT_MS)
      offset = p.offset ?? 0
      position = start
      video.querySelectorAll('track').forEach((t) => t.remove())
      for (const s of p.subtitles) {
        const t = video.appendChild(document.createElement('track'))
        t.kind = 'subtitles'
        t.label = s.label ?? s.language ?? `Piste ${s.n + 1}`
        if (s.language) t.srclang = s.language
        t.src = filmsApi.base + s.url
      }
      showSubtitle()
      h.onTracks?.({ audio: p.audio, subtitles: p.subtitles, audioN: audio, subN: sub, qualities: p.qualities, quality: p.quality ?? quality })
      adapter.load(filmsApi.base + p.url, Math.max(0, start - offset), play)
    } catch (e) {
      if (!gone) h.onError(e instanceof ApiError ? e.message : 'La lecture n’a pas pu démarrer.')
    }
  }
  const repli = (m: string) => {
    clearTimeout(guet)
    if (gone) return
    if (!ref || retried) { h.onError(m); return }
    retried = true
    forceHls = true
    debug.log('info', 'lecteur', 'repli sur le flux transcodé (HLS)')
    void open(position, true)
  }
  const { adapter, video } = htmlVideo(el, {
    ...h,
    onState: (s) => { if (s === 'playing') { retried = false; clearTimeout(guet) } h.onState(s) },
    onError: repli,
  }, true)
  video.addEventListener('loadedmetadata', () => {
    if (stream?.mode === 'direct' && video.videoWidth === 0) {
      debug.log('warn', 'lecteur', 'lecture directe sans piste vidéo décodable')
      repli('Cette vidéo ne peut pas être lue sur cet appareil.')
    }
  })
  video.addEventListener('timeupdate', () => {
    if (video.currentTime > 0) position = absolute(video.currentTime, offset)
    if (stream && saveDue(savedAt, Date.now())) save()
  })
  video.addEventListener('pause', () => { if (stream && !video.seeking) save() })
  const leave = () => { if (stream) filmsApi.closeOnExit(stream.stream_id) }
  window.addEventListener('pagehide', leave)
  const now = () => absolute(video.currentTime, offset)
  return {
    ...adapter,
    time: now,
    duration: total,
    seek: (s) => {
      const ready = Number.isFinite(video.duration) ? video.duration : 0
      if (stream?.mode === 'hls' && stream.offset !== undefined && needsReopen(s, offset, ready)) { position = s; void open(s, !video.paused); return }
      adapter.seek(Math.max(0, s - offset))
    },
    load: (r, start, play) => {
      save()
      ref = r
      position = start
      audio = 0
      offset = 0
      quality = savedQuality(store())
      forceHls = false
      retried = false
      void open(start, play)
    },
    setAudio: (n) => { audio = n; void open(now(), !video.paused) },
    setSubtitle: (n) => { sub = n; showSubtitle() },
    setQuality: (q) => { quality = q; saveQuality(store(), q); void open(now(), !video.paused) },
    destroy: () => { gone = true; clearTimeout(guet); window.removeEventListener('pagehide', leave); save(); close(); adapter.destroy() },
  }
}

// --- Twitch : iframe du lecteur, sans script ni postMessage documenté : seul le lancement est synchronisé ---

const twitchTime = (s: number) => `${Math.floor(s / 3600)}h${Math.floor(s / 60) % 60}m${Math.floor(s % 60)}s`
/** Twitch exige le domaine de la page dans `parent` (dans l'app PC et Android : celui du webview, à vérifier). */
const parent = () => location.hostname || 'localhost'

/** Adresse de l'iframe : rediffusion (`video`, au temps voulu), direct (`channel`) ou clip. */
export function twitchSrc(kind: 'vod' | 'live' | 'clip', ref: string, start: number, play: boolean, host = parent()): string {
  const common = `parent=${encodeURIComponent(host)}&autoplay=${play}`
  if (kind === 'clip') return `https://clips.twitch.tv/embed?clip=${encodeURIComponent(ref)}&${common}`
  if (kind === 'live') return `https://player.twitch.tv/?channel=${encodeURIComponent(ref)}&${common}`
  return `https://player.twitch.tv/?video=v${encodeURIComponent(ref)}&time=${twitchTime(start)}&${common}`
}

function twitch(kind: 'vod' | 'live' | 'clip'): Factory {
  return async (el, h) => {
    const f = document.createElement('iframe')
    f.allowFullscreen = true
    f.allow = 'autoplay; fullscreen'
    f.style.cssText = 'width:100%;height:100%;border:0'
    el.appendChild(f)
    let st: PState = 'unstarted'
    let playing = true
    f.addEventListener('load', () => { st = playing ? 'playing' : 'cued'; h.onState(st) })
    return {
      sync: 'start', canRate: false,
      load: (ref, start, play) => { st = 'unstarted'; playing = play; f.src = twitchSrc(kind, ref, start, play) },
      play() {}, pause() {}, seek() {}, time: () => 0, rate: () => 1, setRate() {},
      state: () => st,
      destroy: () => f.remove(),
    }
  }
}

// --- Vimeo : lecteur intégré piloté par postMessage (protocole de player.js, sans son script) ---

const VIMEO_ORIGIN = 'https://player.vimeo.com'
const VIMEO_EVENTS = ['play', 'playing', 'pause', 'ended', 'timeupdate', 'bufferstart', 'bufferend', 'seeked', 'error']

export interface VimeoInfo { event: string; time?: number; duration?: number; method?: string }

/** Évènement du lecteur Vimeo (`ready`, `timeupdate`, `play`…) ; une réponse à une méthode (`getDuration`) est rendue comme `method`. */
export function readVimeo(data: unknown): VimeoInfo | null {
  const m = parseMessage(data)
  if (!m) return null
  if (typeof m.event === 'string') {
    const i: VimeoInfo = { event: m.event }
    const x = objet(m.data)
    if (x) {
      if (num(x.seconds)) i.time = x.seconds
      if (num(x.duration)) i.duration = x.duration
      if (typeof x.method === 'string') i.method = x.method
    }
    return i
  }
  if (typeof m.method === 'string') return num(m.value) && m.method === 'getDuration' ? { event: 'duration', duration: m.value } : null
  return null
}

const vimeo: Factory = async (el, h) => {
  let st: PState = 'unstarted'
  let t = 0
  let d = 0
  let start = 0
  let autoplay = false
  const set = (s: PState) => { st = s; h.onState(s) }
  const frame = playerFrame(el, VIMEO_ORIGIN, (data) => {
    const i = readVimeo(data)
    if (!i) return
    if (i.time !== undefined) t = i.time
    if (i.duration !== undefined) d = i.duration
    switch (i.event) {
      case 'ready':
        for (const e of VIMEO_EVENTS) send('addEventListener', e)
        send('getDuration')
        if (start > 0) send('setCurrentTime', start)
        set('cued')
        if (autoplay) send('play')
        break
      case 'play': set('buffering'); break
      case 'playing': set('playing'); break
      case 'pause': set('paused'); break
      case 'bufferstart': set('buffering'); break
      case 'bufferend': case 'seeked': set(st === 'buffering' ? 'playing' : st); break
      case 'ended': set('ended'); break
      case 'error':
        if (i.method === 'play') set('paused')
        else if (!i.method) h.onError("Vimeo refuse la lecture de cette vidéo ici (privée ou intégration désactivée).")
        break
    }
  })
  const send = (method: string, value?: unknown) => frame.post(value === undefined ? { method } : { method, value })
  frame.f.addEventListener('load', () => send('ping'))
  return {
    sync: 'full', canRate: false,
    load: (ref, s, play) => {
      st = 'unstarted'
      t = start = s
      d = 0
      autoplay = play
      frame.f.src = `${VIMEO_ORIGIN}/video/${encodeURIComponent(ref)}?api=1&autoplay=0&playsinline=1&dnt=1`
    },
    play: () => send('play'), pause: () => send('pause'), seek: (s) => { t = s; send('setCurrentTime', s) },
    time: () => t, duration: () => d, rate: () => 1, setRate() {}, setVolume: (v) => send('setVolume', v / 100),
    state: () => st,
    destroy: () => frame.remove(),
  }
}

// --- Dailymotion : lecteur intégré piloté par postMessage ---

const dailymotion: Factory = async (el, h) => {
  const f = document.createElement('iframe')
  f.allowFullscreen = true
  f.allow = 'autoplay; fullscreen'
  f.style.cssText = 'width:100%;height:100%;border:0'
  el.appendChild(f)
  const origin = 'https://www.dailymotion.com'
  let st: PState = 'unstarted'
  let t = 0
  let d = 0
  let id = ''
  const send = (m: string) => f.contentWindow?.postMessage(m, origin)
  const set = (s: PState) => { st = s; h.onState(s) }
  const onMsg = (e: MessageEvent) => {
    if (e.origin !== origin || e.source !== f.contentWindow || typeof e.data !== 'string') return
    const q = new URLSearchParams(e.data)
    if (q.get('id') && id && q.get('id') !== id) return
    switch (q.get('event')) {
      case 'apiready': set('cued'); break
      case 'playing': case 'play': set('playing'); break
      case 'pause': set('paused'); break
      case 'waiting': case 'seeking': set('buffering'); break
      case 'seeked': set(st === 'buffering' ? 'playing' : st); break
      case 'end': set('ended'); break
      case 'timeupdate': t = Number(q.get('time')) || t; break
      case 'durationchange': d = Number(q.get('duration')) || d; break
      case 'error': h.onError('Dailymotion refuse la lecture de cette vidéo ici.'); break
    }
  }
  addEventListener('message', onMsg)
  return {
    sync: 'full', canRate: false,
    load: (ref, start, play) => {
      id = Math.random().toString(36).slice(2)
      t = start
      st = 'unstarted'
      f.src = `${origin}/embed/video/${encodeURIComponent(ref)}?api=postMessage&id=${id}&autoplay=${play ? 1 : 0}&start=${Math.floor(start)}&origin=${encodeURIComponent(location.origin)}&queue-enable=0&sharing-enable=0`
    },
    play: () => send('play'), pause: () => send('pause'), seek: (s) => { t = s; send(`seek=${s}`) },
    time: () => t, duration: () => d, rate: () => 1, setRate() {},
    state: () => st,
    destroy: () => { removeEventListener('message', onMsg); f.remove() },
  }
}

export const providers: Record<string, Factory> = {
  youtube, file, film, page: file, vimeo, dailymotion, twitch_vod: twitch('vod'), twitch_live: twitch('live'), twitch_clip: twitch('clip'),
}
