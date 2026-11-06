import { API_URL, filmsApi, ApiError } from '../../lib/api'
import { directOk, saveDue, type FilmPlayback, type FilmTracks } from '../films/films'
import { playerError } from './watch'

/* eslint-disable @typescript-eslint/no-explicit-any */

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

const scripts = new Map<string, Promise<void>>()
/** Charge un script de lecteur officiel une seule fois ; `ready` dit s'il a défini son objet global. */
function loadScript(src: string, ready: () => unknown, name: string): Promise<void> {
  if (ready()) return Promise.resolve()
  let p = scripts.get(src)
  if (!p) {
    p = new Promise<void>((resolve, reject) => {
      const s = document.createElement('script')
      s.src = src
      s.onload = () => (ready() ? resolve() : reject(new Error(`le lecteur ${name} n'a pas pu être initialisé`)))
      s.onerror = () => { scripts.delete(src); reject(new Error(`le lecteur ${name} n'a pas pu être chargé`)) }
      document.head.appendChild(s)
    })
    scripts.set(src, p)
  }
  return p
}

// --- YouTube : lecteur officiel (IFrame API) ---

let ytReady: Promise<any> | null = null
function loadYouTube(): Promise<any> {
  const w = window as any
  if (w.YT?.Player) return Promise.resolve(w.YT)
  ytReady ??= new Promise((resolve, reject) => {
    const prev = w.onYouTubeIframeAPIReady
    w.onYouTubeIframeAPIReady = () => { prev?.(); resolve(w.YT) }
    const s = document.createElement('script')
    s.src = 'https://www.youtube.com/iframe_api'
    s.onerror = () => { ytReady = null; reject(new Error("le lecteur YouTube n'a pas pu être chargé")) }
    document.head.appendChild(s)
  })
  return ytReady
}
const YT_STATE: Record<number, PState> = { [-1]: 'unstarted', 0: 'ended', 1: 'playing', 2: 'paused', 3: 'buffering', 5: 'cued' }

const youtube: Factory = async (el, h) => {
  const YT = await loadYouTube()
  const p: any = await new Promise((resolve) => {
    const pl = new YT.Player(el, {
      width: '100%', height: '100%',
      playerVars: { playsinline: 1, rel: 0, enablejsapi: 1, controls: 0, disablekb: 1, fs: 0, origin: location.origin },
      host: 'https://www.youtube-nocookie.com',
      events: {
        onReady: () => resolve(pl),
        onStateChange: (e: { data: number }) => h.onState(YT_STATE[e.data] ?? 'unstarted'),
        onError: (e: { data: number }) => h.onError(playerError(e.data)),
        onAutoplayBlocked: () => h.onBlocked?.(),
      },
    })
  })
  return {
    sync: 'full', canRate: true,
    load: (ref, start, play) => (play ? p.loadVideoById({ videoId: ref, startSeconds: start }) : p.cueVideoById({ videoId: ref, startSeconds: start })),
    play: () => p.playVideo(), pause: () => p.pauseVideo(), seek: (s) => p.seekTo(s, true),
    time: () => p.getCurrentTime(), duration: () => p.getDuration() || 0, rate: () => p.getPlaybackRate(), setRate: (r) => p.setPlaybackRate(r), setVolume: (v) => p.setVolume(v),
    state: () => YT_STATE[p.getPlayerState()] ?? 'unstarted',
    destroy: () => { try { p.destroy() } catch { /* lecteur déjà retiré */ } },
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
  let hls: any = null
  let buffering = false
  const state = (): PState => {
    if (v.ended) return 'ended'
    if (!v.paused) return buffering || v.readyState < 3 ? 'buffering' : 'playing'
    return v.readyState >= 1 ? 'paused' : 'unstarted'
  }
  const emit = () => h.onState(state())
  v.addEventListener('waiting', () => { buffering = true; emit() })
  for (const ev of ['playing', 'canplay', 'pause', 'ended', 'seeked']) v.addEventListener(ev, () => { if (ev !== 'pause' || !v.seeking) { buffering = false; emit() } })
  v.addEventListener('error', () => h.onError('Cette vidéo ne peut pas être lue (format ou accès refusé par le site qui l\'héberge).'))
  const detach = () => { hls?.destroy(); hls = null }
  const adapter: Adapter = {
    sync: 'full', canRate: true, setVolume: (x) => { v.volume = x / 100; v.muted = x === 0 },
    load: (ref, start, play) => {
      detach()
      buffering = false
      const go = () => { if (start > 0) v.currentTime = start; if (play) void v.play().catch(() => h.onState('paused')) }
      if (isHls(ref) && !v.canPlayType('application/vnd.apple.mpegurl')) {
        void import('hls.js').then(({ default: Hls }) => {
          if (!Hls.isSupported()) return h.onError("Ce flux HLS n'est pas pris en charge par cet appareil.")
          hls = new Hls({ enableWorker: false }) // pas de worker : la CSP n'autorise pas blob: pour les workers
          hls.on(Hls.Events.ERROR, (_: unknown, d: { fatal?: boolean }) => { if (d.fatal) h.onError('Le flux vidéo ne peut pas être lu (réseau ou accès refusé).') })
          hls.on(Hls.Events.MANIFEST_PARSED, go)
          hls.loadSource(ref)
          hls.attachMedia(v)
        }).catch(() => h.onError("Le lecteur HLS n'a pas pu être chargé."))
        return
      }
      v.src = ref
      v.addEventListener('loadedmetadata', go, { once: true })
      v.load()
    },
    play: () => void v.play().catch(() => {}), pause: () => v.pause(), seek: (s) => { v.currentTime = s },
    time: () => v.currentTime, duration: () => (Number.isFinite(v.duration) ? v.duration : 0), rate: () => v.playbackRate, setRate: (r) => { v.playbackRate = r },
    state,
    destroy: () => { detach(); v.pause(); v.removeAttribute('src'); v.load(); v.remove() },
  }
  return { adapter, video: v }
}

const file: Factory = async (el, h) => htmlVideo(el, h).adapter


const film: Factory = async (el, h) => {
  let ref = ''
  let audio = 0
  let sub: number | null = null
  let stream: FilmPlayback | null = null
  let forceHls = false
  let retried = false
  let gone = false
  let opening = 0
  let savedAt = 0
  let position = 0

  const close = () => { if (stream) void filmsApi.close(stream.stream_id).catch(() => {}); stream = null }
  const save = () => {
    const duration = stream?.duration || video.duration
    if (!ref || position <= 0 || !Number.isFinite(duration) || duration <= 0) return
    savedAt = Date.now()
    void filmsApi.progress(ref, position, duration).catch(() => {})
  }
  const showSubtitle = () => { for (let i = 0; i < video.textTracks.length; i++) video.textTracks[i].mode = i === sub ? 'showing' : 'disabled' }
  const open = async (start: number, play: boolean) => {
    const turn = ++opening
    close()
    try {
      const p = await filmsApi.playback(ref, { direct: !forceHls && directOk((t) => video.canPlayType(t)), audio })
      if (gone || turn !== opening) { void filmsApi.close(p.stream_id).catch(() => {}); return }
      stream = p
      video.querySelectorAll('track').forEach((t) => t.remove())
      for (const s of p.subtitles) {
        const t = video.appendChild(document.createElement('track'))
        t.kind = 'subtitles'
        t.label = s.label ?? s.language ?? `Piste ${s.n + 1}`
        if (s.language) t.srclang = s.language
        t.src = API_URL + s.url
      }
      showSubtitle()
      h.onTracks?.({ audio: p.audio, subtitles: p.subtitles, audioN: audio, subN: sub })
      adapter.load(API_URL + p.url, start, play)
    } catch (e) {
      if (!gone) h.onError(e instanceof ApiError ? e.message : 'La lecture n’a pas pu démarrer.')
    }
  }
  const { adapter, video } = htmlVideo(el, {
    ...h,
    onState: (s) => { if (s === 'playing') retried = false; h.onState(s) },
    onError: (m) => {
      if (gone) return
      if (!ref || retried) { h.onError(m); return }
      retried = true
      forceHls = true
      void open(position, true)
    },
  }, true)
  video.addEventListener('timeupdate', () => {
    if (video.currentTime > 0) position = video.currentTime
    if (stream && saveDue(savedAt, Date.now())) save()
  })
  video.addEventListener('pause', () => { if (stream && !video.seeking) save() })
  const leave = () => { if (stream) filmsApi.closeOnExit(stream.stream_id) }
  window.addEventListener('pagehide', leave)
  return {
    ...adapter,
    load: (r, start, play) => {
      save()
      ref = r
      position = start
      audio = 0
      forceHls = false
      retried = false
      void open(start, play)
    },
    setAudio: (n) => { audio = n; void open(video.currentTime, !video.paused) },
    setSubtitle: (n) => { sub = n; showSubtitle() },
    destroy: () => { gone = true; window.removeEventListener('pagehide', leave); save(); close(); adapter.destroy() },
  }
}

// --- Twitch : rediffusions (API du lecteur), directs et clips (lecture simultanée seulement) ---

const twitchTime = (s: number) => `${Math.floor(s / 3600)}h${Math.floor(s / 60) % 60}m${Math.floor(s % 60)}s`
/** Twitch exige le domaine de la page dans `parent` (dans l'app PC et Android : celui du webview, à vérifier). */
const parent = () => location.hostname || 'localhost'

function twitch(kind: 'vod' | 'live'): Factory {
  return async (el, h) => {
    await loadScript('https://player.twitch.tv/js/embed/v1.js', () => (window as any).Twitch?.Player, 'Twitch')
    const T = (window as any).Twitch.Player
    let p: any = null
    let st: PState = 'unstarted'
    const set = (s: PState) => { st = s; h.onState(s) }
    return {
      sync: kind === 'vod' ? 'full' : 'start', canRate: false,
      load: (ref, start, play) => {
        const video = kind === 'vod' ? `v${ref}` : ''
        if (!p) {
          p = new T(el, { width: '100%', height: '100%', parent: [parent()], autoplay: play, ...(kind === 'vod' ? { video, time: twitchTime(start) } : { channel: ref }) })
          p.addEventListener(T.READY, () => set('cued'))
          p.addEventListener(T.PLAYING, () => set('playing'))
          p.addEventListener(T.PAUSE, () => set('paused'))
          p.addEventListener(T.ENDED, () => set('ended'))
          p.addEventListener(T.OFFLINE, () => h.onError("Ce direct n'est pas en ligne."))
          return
        }
        if (kind === 'vod') p.setVideo(video, start)
        else p.setChannel(ref)
        if (play) p.play()
      },
      play: () => p?.play(), pause: () => p?.pause(), seek: (s) => p?.seek(s),
      time: () => (p ? p.getCurrentTime() : 0), duration: () => (kind === 'vod' && p ? p.getDuration() || 0 : 0), rate: () => 1, setRate: () => {},
      state: () => st,
      destroy: () => { try { p?.destroy?.() } catch { /* retiré */ } el.replaceChildren() },
    }
  }
}

const twitchClip: Factory = async (el, h) => {
  const f = document.createElement('iframe')
  f.allowFullscreen = true
  f.allow = 'autoplay; fullscreen'
  f.style.cssText = 'width:100%;height:100%;border:0'
  el.appendChild(f)
  let st: PState = 'unstarted'
  f.addEventListener('load', () => { st = 'playing'; h.onState(st) })
  return {
    sync: 'start', canRate: false,
    load: (ref) => { st = 'unstarted'; f.src = `https://clips.twitch.tv/embed?clip=${encodeURIComponent(ref)}&parent=${encodeURIComponent(parent())}&autoplay=true` },
    play() {}, pause() {}, seek() {}, time: () => 0, rate: () => 1, setRate() {},
    state: () => st,
    destroy: () => f.remove(),
  }
}

// --- Vimeo : lecteur officiel (player.js) ---

const vimeo: Factory = async (el, h) => {
  await loadScript('https://player.vimeo.com/api/player.js', () => (window as any).Vimeo?.Player, 'Vimeo')
  const V = (window as any).Vimeo.Player
  let p: any = null
  let st: PState = 'unstarted'
  let t = 0
  let d = 0
  const set = (s: PState) => { st = s; h.onState(s) }
  const poll = setInterval(() => { if (p) { void p.getCurrentTime().then((x: number) => (t = x)).catch(() => {}); void p.getDuration().then((x: number) => (d = x)).catch(() => {}) } }, 250)
  return {
    sync: 'full', canRate: false,
    load: (ref, start, play) => {
      if (!p) {
        p = new V(el, { id: Number(ref), autoplay: false, playsinline: true, width: el.clientWidth || 640 })
        p.on('loaded', () => { if (start > 0) void p.setCurrentTime(start); set('cued'); if (play) void p.play().catch(() => set('paused')) })
        p.on('play', () => set('buffering'))
        p.on('playing', () => set('playing'))
        p.on('pause', () => set('paused'))
        p.on('bufferstart', () => set('buffering'))
        p.on('bufferend', () => set(st === 'buffering' ? 'playing' : st))
        p.on('ended', () => set('ended'))
        p.on('error', () => h.onError("Vimeo refuse la lecture de cette vidéo ici (privée ou intégration désactivée)."))
        return
      }
      void p.loadVideo(Number(ref)).then(() => { if (start > 0) void p.setCurrentTime(start); if (play) void p.play().catch(() => set('paused')) }).catch(() => h.onError('Vidéo Vimeo introuvable ou privée.'))
    },
    play: () => void p?.play().catch(() => {}), pause: () => void p?.pause(), seek: (s) => { t = s; void p?.setCurrentTime(s) },
    time: () => t, duration: () => d, rate: () => 1, setRate() {},
    state: () => st,
    destroy: () => { clearInterval(poll); try { void p?.destroy() } catch { /* retiré */ } },
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
  youtube, file, film, page: file, vimeo, dailymotion, twitch_vod: twitch('vod'), twitch_live: twitch('live'), twitch_clip: twitchClip,
}
