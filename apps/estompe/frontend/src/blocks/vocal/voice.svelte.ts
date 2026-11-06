import { api, ApiError } from '../../lib/api'
import { applyCameraFx } from '../../lib/camerafx'
import { prefs, type ScreenRes, type ScreenFps, type CameraQuality } from '../../lib/prefs.svelte'
import { playTone } from '../notifications/notify'
import { watchSpeaking } from './speaking'
import { collectWatchers } from './stage'
import { gainOf, ownerOf } from '../../lib/volume'
import { shouldSubscribe } from './stage'
import { notify } from '../../lib/ui'
import { platform, type CaptureSource } from '../../lib/platform'
import { debug } from '../../lib/debuglog.svelte'
import type { Room, Track as LkTrack, LocalVideoTrack, AudioCaptureOptions, VideoCaptureOptions, TrackPublishOptions } from 'livekit-client'

/** Résolutions du partage d'écran ; le débit est donné pour 30 images/s (voir screenBitrate). */
export const SCREEN_RES: Record<ScreenRes, { width: number; height: number; bitrate: number }> = {
  '720p': { width: 1280, height: 720, bitrate: 1_500_000 },
  '1080p': { width: 1920, height: 1080, bitrate: 3_000_000 },
  '1440p': { width: 2560, height: 1440, bitrate: 7_000_000 },
  '2160p': { width: 3840, height: 2160, bitrate: 12_000_000 }, // 4K : ~12 Mb/s montants, et autant par spectateur côté serveur
}
export const SCREEN_FPS: ScreenFps[] = [30, 60, 120]
const RES_ORDER = Object.keys(SCREEN_RES) as ScreenRes[]

export const screenBitrate = (r: ScreenRes, fps: ScreenFps) => Math.round(SCREEN_RES[r].bitrate * (fps === 30 ? 1 : fps === 60 ? 1.7 : 2.5))

/** Ramène le choix aux plafonds annoncés par le serveur. */
export function clampScreen(res: ScreenRes, fps: ScreenFps, max: { fps: number; height: number }): { res: ScreenRes; fps: ScreenFps } {
  const okRes = RES_ORDER.filter((r) => SCREEN_RES[r].height <= max.height)
  const ri = Math.min(RES_ORDER.indexOf(res), Math.max(0, okRes.length - 1))
  const okFps = SCREEN_FPS.filter((f) => f <= max.fps)
  return { res: RES_ORDER[ri], fps: okFps.includes(fps) ? fps : (okFps.at(-1) ?? 30) }
}

/** Palier inférieur quand la connexion ou le processeur ne suivent pas : d'abord les images/s, puis la résolution. */
export function stepDown(c: { res: ScreenRes; fps: ScreenFps }): { res: ScreenRes; fps: ScreenFps } | null {
  if (c.fps > 30) return { res: c.res, fps: c.fps === 120 ? 60 : 30 }
  const i = RES_ORDER.indexOf(c.res)
  return i > 0 ? { res: RES_ORDER[i - 1], fps: 30 } : null
}

export const CAMERA: Record<CameraQuality, { width: number; height: number; fps: number; bitrate: number }> = {
  '360p': { width: 640, height: 360, fps: 30, bitrate: 700_000 },
  '720p': { width: 1280, height: 720, fps: 30, bitrate: 2_200_000 },
  '1080p': { width: 1920, height: 1080, fps: 30, bitrate: 4_000_000 },
}

function cameraOptions(): { capture: VideoCaptureOptions; publish: TrackPublishOptions } {
  const q = CAMERA[prefs.cameraQuality]
  return {
    capture: { deviceId: prefs.cameraId || undefined, resolution: { width: q.width, height: q.height, frameRate: q.fps } },
    // Plusieurs couches (simulcast) : chaque spectateur reçoit celle que sa connexion et la taille de la tuile permettent.
    publish: {
      simulcast: true,
      videoEncoding: { maxBitrate: q.bitrate, maxFramerate: q.fps },
      videoCodec: prefs.videoCodec === 'auto' ? undefined : prefs.videoCodec,
      backupCodec: true,
      degradationPreference: 'balanced',
    },
  }
}

/** Qualité de connexion d'un participant, telle que rapportée par LiveKit. */
export type Quality = 'excellent' | 'good' | 'poor' | 'lost' | 'unknown'

/** Micro toujours en mono : un micro stéréo dont la voix n'est que sur le canal gauche serait sinon publié tel quel (EST-86). */
export function audioConstraints(): AudioCaptureOptions {
  const on = prefs.noise !== 'off'
  return { deviceId: prefs.micId || undefined, channelCount: 1, echoCancellation: on, noiseSuppression: on, autoGainControl: on }
}

export type VoiceState = 'idle' | 'connecting' | 'connected'

/** Piste vidéo affichable : caméra ou partage d'écran, de soi ou d'un autre participant. */
export interface VideoTile {
  sid: string
  userId: string
  name: string
  screen: boolean
  /** Absent tant qu'un partage d'écran distant n'est pas regardé : rien n'est reçu. */
  track?: LkTrack
}

/** Salon vocal : connexion à LiveKit (chargé à la demande pour ne pas alourdir le démarrage). */
/** Message affiché quand le micro ne peut pas être ouvert : l'utilisateur reste dans le salon en écoute seule. */
export function micError(e: unknown): string {
  return `${micCause(e)} Vous êtes en écoute seule.`
}

/** Cause lisible d'un échec d'ouverture du micro (sans la suite propre au salon vocal). */
export function micCause(e: unknown): string {
  const name = e instanceof DOMException ? e.name : ''
  return name === 'NotAllowedError' || name === 'SecurityError' ? `Accès au micro refusé.${platform.micHelp}`
    : name === 'NotFoundError' || name === 'OverconstrainedError' ? 'Aucun micro disponible (vérifiez le micro choisi dans les Paramètres).'
    : 'Le micro est inutilisable.'
}

const errText = (e: unknown) => (e instanceof Error ? `${e.name} : ${e.message}` : String(e))

/** Mode debug : environnement audio au moment de rejoindre (navigateur, PWA, permission micro, périphériques), sans identifiants ni noms de périphériques. */
async function logAudioEnv() {
  if (!debug.enabled) return
  debug.log('info', 'vocal', `environnement : ${navigator.userAgent} ; ${platform.label()} ; contexte sécurisé ${window.isSecureContext}`)
  try {
    const perm = await navigator.permissions.query({ name: 'microphone' as PermissionName })
    debug.log('info', 'vocal', `permission micro : ${perm.state}`)
  } catch (e) {
    debug.log('info', 'vocal', `permission micro illisible : ${errText(e)}`)
  }
  try {
    const list = await navigator.mediaDevices.enumerateDevices()
    const n = (k: string) => list.filter((d) => d.kind === k).length
    const has = (k: string, id: string) => (id ? (list.some((d) => d.kind === k && d.deviceId === id) ? 'présent' : 'ABSENT') : 'défaut')
    debug.log('info', 'vocal', `périphériques : ${n('audioinput')} entrée(s), ${n('audiooutput')} sortie(s) ; micro choisi ${has('audioinput', prefs.micId)}, sortie choisie ${has('audiooutput', prefs.speakerId)}`)
  } catch (e) {
    debug.log('warn', 'vocal', `périphériques illisibles : ${errText(e)}`)
  }
}

/** Raccourcis du vocal, actifs quand la fenêtre est au premier plan (le push-to-talk a son propre réglage). */
/** État micro / sourdine affiché : qui parle n'a pas le micro coupé, quel que soit l'attribut (périmé après un évènement perdu). */
export function voiceStates(ps: { identity: string; attributes: Record<string, string> }[], speaking: Record<string, true>) {
  const st: Record<string, { muted: boolean; deaf: boolean }> = {}
  for (const p of ps) st[p.identity] = { muted: !!p.attributes.muted && !speaking[p.identity], deaf: !!p.attributes.deaf }
  return st
}

export const SHORTCUT = { mute: 'Alt+Maj+M', deafen: 'Alt+Maj+D', camera: 'Alt+Maj+V', screen: 'Alt+Maj+S' }

class VoiceController {
  state = $state<VoiceState>('idle')
  channelId = $state<string | null>(null)
  muted = $state(false)
  error = $state('')
  /** Identités (ids d'utilisateurs) en train de parler. */
  speaking = $state<Record<string, true>>({})
  private meters = new Map<string, () => void>()
  camera = $state(false)
  /** Fenêtre de choix ouverte (écran ou caméra) ; affichée par MediaPicker. */
  picker = $state<'screen' | 'camera' | null>(null)
  /** Flux de l'écran choisi dans le sélecteur du système, montré en aperçu avant d'être publié. */
  preview = $state.raw<MediaStream | null>(null)
  /** Étape de la fenêtre de partage : choix de la source (app Windows, alpha : liste maison), puis aperçu et qualité. */
  step = $state<'source' | 'preview'>('preview')
  /** Source choisie dans la liste maison (alpha, EST-18), publiée par Rust ; `nativeShare` : ce partage est en cours. */
  nativeSource = $state.raw<CaptureSource | null>(null)
  private nativeShare = false
  /** Hauteur native de l'écran partagé (une capture ne peut pas dépasser sa source), null si inconnue. */
  sourceHeight = $state<number | null>(null)
  /** Plafonds du partage d'écran annoncés par le serveur. */
  limits = $state({ fps: 60, height: 2160 })
  private adapt: ReturnType<typeof setInterval> | undefined
  screen = $state(false)
  /** Pistes vidéo publiées dans la salle (caméras et écrans partagés). */
  tiles = $state.raw<VideoTile[]>([])

  /** Sourdine casque : plus aucun son reçu, et le micro est coupé. */
  deafened = $state(false)
  /** États de la salle (micro coupé, sourd), publiés par chaque participant. */
  /** Participants dont le partage d'écran a du son (le curseur « Partage d'écran » n'a de sens que pour eux). */
  screenAudio = $state<Record<string, boolean>>({})
  states = $state<Record<string, { muted: boolean; deaf: boolean }>>({})
  private mutedBeforeDeaf = false
  /** Spectateurs de chaque flux (par `sid` de piste), d'après l'attribut `watching` publié par chaque participant. */
  watchers = $state<Record<string, { id: string; name: string }[]>>({})
  /** Streams (par id du propriétaire) que l'on a choisi de regarder (true) ou d'arrêter (false) ; sinon `prefs.watchAuto`. */
  watched = $state<Record<string, boolean>>({})
  private viewing = new Map<string, number>()
  /** Libère le raccourci global du push-to-talk (app Windows) ; défini tant qu'il est enregistré. */
  private offGlobal: (() => Promise<void>) | null = null
  private globalSeq = 0
  /** Touche push-to-talk actuellement enfoncée. */
  talking = $state(false)
  /** Détection de voix : vrai tant que le niveau du micro dépasse le seuil (avec un court délai de maintien). */
  voiceActive = $state(false)
  /** Niveau du micro en dBFS (de -100 à 0), mis à jour pendant un appel. */
  level = $state(-100)
  quality = $state<Record<string, Quality>>({})
  /** Tuile affichée en grand. */
  pinned = $state<string | null>(null)

  private room: Room | null = null
  private audio: HTMLElement[] = []
  /** Dernier salon quitté : le retrait par le serveur peut précéder l'évènement de déplacement. */
  dropped: { id: string; at: number } | null = null
  private offKeys: (() => void) | null = null
  private processor: { setGain(v: number): void; setGate(open: boolean): void; level(): number } | null = null
  private poll: ReturnType<typeof setInterval> | null = null
  private lastAbove = 0

  async join(channelId: string, opts: { camera?: boolean; swap?: boolean } = {}) {
    if (this.channelId === channelId && this.state !== 'idle') return
    // déplacement : l'ancienne salle n'est quittée qu'une fois la nouvelle connectée (jamais « déconnecté » entre les deux)
    const old = opts.swap ? this.room : null
    if (!old) await this.leave()
    this.error = ''
    this.state = 'connecting'
    this.channelId = channelId
    let attempt: Room | undefined
    try {
      const [{ Room, RoomEvent, Track }, t] = await Promise.all([import('livekit-client'), api.voiceToken(channelId)])
      const room = new Room({
        adaptiveStream: true, // la résolution reçue suit la taille de la tuile (grande tuile = haute qualité)
        dynacast: true, // les couches que personne ne regarde ne sont pas encodées
        audioCaptureDefaults: audioConstraints(),
        videoCaptureDefaults: cameraOptions().capture,
        webAudioMix: true, // nécessaire pour dépasser 100 % de volume d'écoute
        audioOutput: { deviceId: prefs.speakerId || undefined },
      })
      this.room = attempt = room
      room
        .on(RoomEvent.TrackSubscribed, (track, _pub, p) => {
          if (track.kind === Track.Kind.Video) return this.syncTiles()
          if (track.kind !== Track.Kind.Audio) return
          if (track.source === Track.Source.Microphone) this.watchVoice(track.sid ?? '', track.mediaStreamTrack, p.identity)
          this.applyVolumes()
          const el = track.attach()
          el.style.display = 'none'
          document.body.append(el)
          this.audio.push(el)
        })
        .on(RoomEvent.TrackUnsubscribed, (track) => {
          this.meters.get(track.sid ?? '')?.()
          track.detach().forEach((el) => el.remove())
          if (track.kind === Track.Kind.Video) this.syncTiles(track)
        })
        .on(RoomEvent.TrackPublished, () => this.syncSubs())
        .on(RoomEvent.TrackUnpublished, (pub, p) => {
          if (pub.source === Track.Source.ScreenShare) delete this.watched[ownerOf(p.identity)]
          this.syncTiles()
          this.applyVolumes()
        })
        .on(RoomEvent.ParticipantConnected, () => {
          this.syncSubs()
          if (prefs.voiceSounds) playTone('join')
          this.syncStates()
          this.applyVolumes()
        })
        .on(RoomEvent.ParticipantAttributesChanged, (changed) => { this.syncStates(); if ('overlay' in changed) this.syncSubs() }) // syncSubs recrée les tuiles, qui republient `watching` : sur chaque attribut, tous les clients se relancent en boucle
        .on(RoomEvent.ConnectionQualityChanged, (q, p) => {
          this.quality[p.identity] = q as Quality
        })
        .on(RoomEvent.ParticipantDisconnected, () => {
          this.syncStates()
          this.syncTiles()
          if (prefs.voiceSounds) playTone('leave')
        })
        .on(RoomEvent.LocalTrackPublished, (pub) => {
          if (pub.source === Track.Source.Microphone && pub.track) this.watchVoice(pub.trackSid, pub.track.mediaStreamTrack, room.localParticipant.identity)
          this.syncTiles()
        })
        .on(RoomEvent.LocalTrackUnpublished, (pub) => {
          this.meters.get(pub.trackSid)?.()
          if (pub.source === Track.Source.ScreenShare) { this.stopAdapt(); this.stopNative(); void this.applyOverlay() }
          this.camera = !!room.localParticipant.isCameraEnabled
          this.screen = !!room.localParticipant.isScreenShareEnabled || this.nativeShare
          this.syncTiles()
        })
        .on(RoomEvent.TrackMuted, (pub, p) => {
          // LiveKit coupe la caméra en la mettant en sourdine sans la dépublier : le serveur ne verrait jamais la fin
          // (webhook « track_unpublished »), l'icône resterait. On la dépublie (coupure, périphérique débranché, permission retirée).
          if (p.isLocal && pub.source === Track.Source.Camera) { const t = room.localParticipant.getTrackPublication(Track.Source.Camera)?.track; if (t) void room.localParticipant.unpublishTrack(t) }
          this.syncTiles()
        })
        .on(RoomEvent.TrackUnmuted, () => this.syncTiles())
        .on(RoomEvent.Disconnected, () => { if (this.room === room) this.reset() }) // une salle quittée (déplacement, nouvelle connexion) ne coupe pas la suivante
      this.limits = { fps: t.screen_max_fps ?? 60, height: t.screen_max_height ?? 2160 }
      debug.log('info', 'vocal', `connexion au salon ${channelId}`)
      room
        .on(RoomEvent.ConnectionStateChanged, (st) => debug.log('info', 'vocal', `état de connexion : ${st}`))
        .on(RoomEvent.SignalConnected, () => debug.log('info', 'vocal', 'signalisation établie'))
        .on(RoomEvent.Reconnecting, () => debug.log('warn', 'vocal', 'reconnexion en cours'))
        .on(RoomEvent.Reconnected, () => {
          debug.log('info', 'vocal', 'reconnecté')
          this.publishState() // une reconnexion complète perd les attributs : on republie micro et sourdine
          this.syncStates()
        })
        .on(RoomEvent.MediaDevicesError, (e) => debug.log('error', 'vocal', `erreur périphérique média : ${errText(e)}`))
        .on(RoomEvent.Disconnected, (reason) => debug.log('warn', 'vocal', `déconnecté (raison ${reason ?? 'inconnue'})`))
      void logAudioEnv()
      await room.connect(t.url, t.token, { autoSubscribe: false }) // un partage d'écran n'est reçu qu'à la demande (syncSubs)
      this.syncSubs()
      debug.log('info', 'vocal', 'connecté')
      if (old) await old.disconnect().catch(() => {})
      this.muted = false
      this.deafened = false
      this.talking = false
      try {
        debug.log('info', 'vocal', 'ouverture du micro')
        await this.enableMic()
        debug.log('info', 'vocal', 'micro publié')
      } catch (e) {
        // micro inutilisable : on reste dans le salon en écoute seule plutôt que de quitter puis retenter
        debug.log('error', 'vocal', `micro indisponible, écoute seule : ${errText(e)}`)
        this.muted = true
        this.error = micError(e)
      }
      this.listenKeys()
      this.startPoll()
      this.state = 'connected'
      this.publishState()
      this.syncStates()
      if (prefs.voiceSounds) playTone('join')
      if (opts.camera) await this.toggleCamera()
    } catch (e) {
      if (attempt && this.room !== attempt) return // tentative périmée (une autre a pris le relais) : ne pas couper la nouvelle salle
      console.warn('connexion au vocal impossible', e)
      debug.log('error', 'vocal', `connexion impossible : ${errText(e)}`)
      await this.leave()
      this.error =
        e instanceof ApiError && e.code === 'voice_full' ? 'Salon vocal complet (10 participants maximum).'
        : e instanceof ApiError ? e.message
        : 'Connexion au vocal impossible.'
    }
  }

  /** Rejoint un autre salon (déplacé par un modérateur) en gardant son état micro et son. */
  async move(channelId: string) {
    const { muted, deafened } = this
    await this.join(channelId, { swap: this.state === 'connected' })
    if (this.state !== 'connected') return
    if (deafened) await this.toggleDeafen()
    else if (muted) await this.toggleMute()
  }

  /** Publie le micro (avec RNNoise en mode renforcé), puis applique muet / push-to-talk. */
  private async enableMic() {
    const lp = this.room?.localParticipant
    if (!lp) return
    await lp.setMicrophoneEnabled(true, audioConstraints())
    this.processor = null
    if (prefs.noise === 'strong' || prefs.micGain !== 100 || prefs.vad) {
      try {
        const { Track } = await import('livekit-client')
        const { MicProcessor } = await import('../../lib/micprocessor')
        const p = new MicProcessor(prefs.noise === 'strong', prefs.micGain / 100)
        const pub = lp.getTrackPublication(Track.Source.Microphone)
        await pub?.audioTrack?.setProcessor(p)
        this.processor = p
        // la piste émise est maintenant la piste traitée : c'est elle qu'il faut mesurer
        if (pub?.audioTrack) this.watchVoice(pub.trackSid, pub.audioTrack.mediaStreamTrack, lp.identity)
      } catch (e) {
        debug.log('warn', 'vocal', `traitement du micro indisponible : ${errText(e)}`)
      }
    }
    await this.applyMic()
  }

  /** Le micro émet si on n'est pas muet et, en push-to-talk, tant que la touche est enfoncée. */
  private async applyMic() {
    const lp = this.room?.localParticipant
    if (!lp) return
    const { Track } = await import('livekit-client')
    const t = lp.getTrackPublication(Track.Source.Microphone)?.audioTrack
    if (!t) return
    const open = prefs.ptt ? this.talking : true
    // détection de voix : la porte du traitement coupe le son sans arrêter la mesure du niveau
    this.processor?.setGate(prefs.ptt || !prefs.vad || this.voiceActive)
    if (!this.muted && open) await t.unmute()
    else await t.mute()
  }

  async toggleMute() {
    if (!this.room) return
    if (this.deafened) return void (await this.toggleDeafen()) // réactiver le micro lève aussi la sourdine
    this.muted = !this.muted
    await this.applyMic()
    this.publishState()
  }

  /** Sourdine casque : coupe le son reçu et le micro ; au retour, le micro retrouve son état d'avant. */
  async toggleDeafen() {
    if (!this.room) return
    this.deafened = !this.deafened
    if (this.deafened) {
      this.mutedBeforeDeaf = this.muted
      this.muted = true
    } else {
      this.muted = this.mutedBeforeDeaf
    }
    this.applyVolumes()
    await this.applyMic()
    this.publishState()
  }

  private syncCall() {
    platform.voiceCall?.update({ channelId: this.state === 'connected' ? this.channelId : null, muted: this.muted, deafened: this.deafened })
  }

  private publishState() {
    this.syncCall()
    this.room?.localParticipant.setAttributes({ muted: this.muted ? '1' : '', deaf: this.deafened ? '1' : '' })
      .catch((e) => debug.log('error', 'vocal', `état du micro non publié : ${errText(e)}`))
  }

  /** Déclare que la tuile `sid` est affichée chez nous (vers les autres participants) ; renvoie la fonction qui l'efface. */
  view(sid: string) {
    this.viewing.set(sid, (this.viewing.get(sid) ?? 0) + 1)
    this.publishWatching()
    return () => {
      const n = (this.viewing.get(sid) ?? 1) - 1
      if (n > 0) this.viewing.set(sid, n)
      else this.viewing.delete(sid)
      this.publishWatching()
    }
  }

  private watching = ''
  private publishWatching() {
    const w = [...this.viewing.keys()].join(',')
    if (w === this.watching) return
    this.watching = w
    void this.room?.localParticipant.setAttributes({ watching: w })
  }

  /** Spectateurs de tous les flux d'un participant (caméra et écran), sans doublon. */
  watchersOf(userId: string) {
    const seen = new Map<string, { id: string; name: string }>()
    for (const t of this.tiles) if (t.userId === userId) for (const w of this.watchers[t.sid] ?? []) seen.set(w.id, w)
    return [...seen.values()]
  }

  private syncStates() {
    const room = this.room
    if (!room) return
    this.watchers = collectWatchers([room.localParticipant, ...room.remoteParticipants.values()].filter((p) => !p.identity.endsWith('#screen')).map((p) => ({ id: p.identity, name: p.name || p.identity, watching: p.attributes.watching })))
    this.states = voiceStates([room.localParticipant, ...room.remoteParticipants.values()], this.speaking)
  }

  /** À appeler quand le mode push-to-talk ou la détection de voix change pendant un appel. */
  async refreshPtt() {
    void this.refreshGlobalPtt()
    this.talking = false
    this.voiceActive = false
    // la détection de voix a besoin du traitement du micro (mesure du niveau)
    if (prefs.vad && !this.processor && this.room) return this.reconfigureMic()
    await this.applyMic()
  }

  /** Mesure le niveau du micro ~15 fois par seconde (jauge) et ouvre / ferme le micro en détection de voix. */
  private startPoll() {
    this.stopPoll()
    let tick = 0
    this.poll = setInterval(() => {
      if (tick++ % 30 === 0) this.syncQuality() // toutes les 2 s environ
      const db = this.processor?.level() ?? -100
      const r = Math.round(db)
      if (r !== this.level) this.level = r
      if (!prefs.vad || prefs.ptt || this.muted) return
      const now = Date.now()
      if (db > prefs.vadThreshold) this.lastAbove = now
      const active = now - this.lastAbove < 400 // maintien : pas de coupure entre deux mots
      if (active !== this.voiceActive) {
        this.voiceActive = active
        this.processor?.setGate(active)
      }
    }, 66)
  }

  /** Lit la qualité de connexion de chacun (l'évènement de LiveKit n'arrive qu'aux changements). */
  private syncQuality() {
    const room = this.room
    if (!room) return
    const q: Record<string, Quality> = {}
    for (const p of [room.localParticipant, ...room.remoteParticipants.values()]) q[p.identity] = p.connectionQuality as Quality
    this.quality = q
  }

  private stopPoll() {
    if (this.poll) clearInterval(this.poll)
    this.poll = null
    this.level = -100
  }

  /** Gain du micro (0 à 300 %), appliqué en direct. */
  async setMicGain() {
    if (this.processor) this.processor.setGain(prefs.micGain / 100)
    else await this.reconfigureMic()
  }

  /** Change de mode de bruit ou de micro pendant un appel : le micro est republié. */
  async reconfigureMic() {
    if (!this.room) return
    await this.room.localParticipant.setMicrophoneEnabled(false)
    await this.enableMic()
  }

  async switchDevice(kind: 'audioinput' | 'audiooutput' | 'videoinput') {
    const id = kind === 'audioinput' ? prefs.micId : kind === 'audiooutput' ? prefs.speakerId : prefs.cameraId
    if (!this.room) return
    try {
      if (kind === 'audioinput') await this.reconfigureMic()
      else await this.room.switchActiveDevice(kind, id || 'default')
    } catch {
      this.error = 'Impossible de changer de périphérique.'
    }
  }

  /** Volume d'écoute (0 à 300 %) de la voix ou du son de partage d'un participant, mémorisé. */
  setUserVolume(userId: string, volume: number, screen = false) {
    ;(screen ? prefs.screenVolumes : prefs.volumes)[userId] = volume
    prefs.save()
    this.applyVolumes()
  }

  /** Modération du créateur : micro et/ou son coupés pour tout le salon. Les erreurs vont au journal de debug. */
  async moderate(cid: string, uid: string, mod: { mic_off: boolean; sound_off: boolean }) {
    try {
      await api.voiceModerate(cid, uid, mod)
    } catch (e) {
      debug.log('error', 'vocal', `modération refusée : ${errText(e)}`)
    }
  }

  /** Sourdine de la voix ou du son de partage d'un participant, mémorisée. */
  setUserMuted(userId: string, muted: boolean, screen = false) {
    ;(screen ? prefs.mutedScreen : prefs.mutedVoice)[userId] = muted
    prefs.save()
    this.applyVolumes()
  }

  private applyVolumes() {
    const screenAudio: Record<string, boolean> = {}
    this.room?.remoteParticipants.forEach((p) => {
      p.setVolume(gainOf(this.deafened, { volumes: prefs.volumes, muted: prefs.mutedVoice }, p.identity))
      p.setVolume(gainOf(this.deafened, { volumes: prefs.screenVolumes, muted: prefs.mutedScreen }, ownerOf(p.identity)), 'screen_share_audio' as never)
      screenAudio[ownerOf(p.identity)] = screenAudio[ownerOf(p.identity)] || !!p.getTrackPublication('screen_share_audio' as never)
    })
    this.screenAudio = screenAudio
  }

  private listenKeys() {
    const typing = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null
      return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable)
    }
    const set = (on: boolean) => {
      if (!prefs.ptt || this.talking === on) return
      this.talking = on
      void this.applyMic()
    }
    const down = (e: KeyboardEvent) => {
      if (typing(e)) return
      if (e.code === prefs.pttKey) set(true)
      if (!e.altKey || !e.shiftKey || e.ctrlKey || e.metaKey) return
      const act = { KeyM: () => this.toggleMute(), KeyD: () => this.toggleDeafen(), KeyV: () => this.requestCamera(), KeyS: () => this.requestScreen() }[e.code]
      if (act) { e.preventDefault(); void act() }
    }
    const up = (e: KeyboardEvent) => {
      if (e.code === prefs.pttKey) set(false)
    }
    // Fenêtre au premier plan : les évènements clavier suffisent (et la touche reste utilisable dans les champs de saisie).
    // Sinon, app Windows : raccourci global, pour parler depuis un jeu ou une autre fenêtre.
    const blur = () => { set(false); void this.syncGlobalPtt(set) }
    const focus = () => void this.syncGlobalPtt(set)
    addEventListener('keydown', down)
    addEventListener('keyup', up)
    addEventListener('blur', blur)
    addEventListener('focus', focus)
    this.setGlobalPtt = () => this.syncGlobalPtt(set)
    void this.syncGlobalPtt(set)
    this.offKeys = () => {
      removeEventListener('focus', focus)
      this.setGlobalPtt = null
      void this.syncGlobalPtt(set, true)
      removeEventListener('keydown', down)
      removeEventListener('keyup', up)
      removeEventListener('blur', blur)
    }
  }

  private setGlobalPtt: (() => Promise<void>) | null = null

  /** Enregistre ou libère le raccourci global selon l'état : salon vocal, push-to-talk, fenêtre sans focus. */
  private async syncGlobalPtt(set: (on: boolean) => void, off = false) {
    const seq = ++this.globalSeq
    const release = this.offGlobal
    this.offGlobal = null
    await release?.()
    if (off || !platform.globalPtt || !prefs.ptt || document.hasFocus() || seq !== this.globalSeq) return
    try {
      const unregister = await platform.globalPtt.register(prefs.pttKey, set)
      if (seq === this.globalSeq) this.offGlobal = unregister
      else await unregister()
    } catch (e) {
      notify.info((e as Error).message)
    }
  }

  /** À appeler quand la touche ou le mode push-to-talk change pendant un appel. */
  refreshGlobalPtt() { return this.setGlobalPtt?.() }

  async toggleCamera() {
    await this.toggleVideo('camera')
  }

  /** Boutons de la barre : éteint directement, sinon ouvre la fenêtre de choix. */
  requestCamera() {
    if (this.camera) void this.toggleVideo('camera')
    else this.picker = 'camera'
  }

  requestScreen() {
    if (this.screen) void this.toggleVideo('screen')
    else void this.pickScreen()
  }

  /** Ouvre le sélecteur du système (écran, fenêtre ou onglet), puis la fenêtre d'aperçu : rien n'est publié avant « Partager ». */
  /** Dans l'app Windows, la liste maison (écrans, fenêtres, aperçus) est la méthode par défaut ; `'system'` force le sélecteur du système en repli. */
  async pickScreen(mode?: 'system' | 'alpha') {
    const own = !!platform.capture && mode !== 'system'
    debug.log('info', 'partage', `sélecteur ${own ? 'Estompe' : 'système'}`)
    this.error = ''
    this.dropPreview()
    if (own) {
      this.sourceHeight = null
      this.step = 'source'
      this.picker = 'screen'
      return
    }
    this.sourceHeight = null // inconnue tant que l'écran n'est pas choisi : seuls les plafonds du serveur s'appliquent
    const { res, fps } = clampScreen(prefs.screenRes, prefs.screenFps, this.maxScreen)
    const q = SCREEN_RES[res]
    try {
      const s = await navigator.mediaDevices.getDisplayMedia({
        video: { width: { ideal: q.width }, height: { ideal: q.height }, frameRate: { ideal: fps } },
        audio: prefs.screenAudio,
      })
      const v = s.getVideoTracks()[0]
      v.contentHint = 'detail'
      this.setSource(v)
      v.addEventListener('ended', () => { if (this.preview === s) this.closePicker() }) // « Arrêter le partage » de la barre du navigateur
      this.preview = s
      this.step = 'preview'
      this.picker = 'screen'
    } catch (e) {
      // Annuler le sélecteur n'est pas une erreur à afficher.
      debug.log('warn', 'partage', `sélecteur système : ${e instanceof Error ? `${e.name} ${e.message}` : String(e)}`)
      if (!(e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'AbortError'))) this.error = "Partage d'écran impossible."
    }
  }

  /** Plafonds réels : ceux du serveur, et la taille de l'écran partagé. */
  get maxScreen() {
    return { fps: this.limits.fps, height: Math.min(this.limits.height, this.sourceHeight ?? Infinity) }
  }

  private setSource(t: MediaStreamTrack | undefined) {
    const h = t?.getCapabilities?.().height?.max // capacités de la source ; getSettings() ne dirait que la taille actuelle, déjà réduite par nos contraintes
    this.sourceHeight = h && h > 0 ? h : null
  }

  /** Source choisie dans la liste maison (alpha) : rien n'est capturé avant « Partager ». */
  chooseSource(src: CaptureSource) {
    this.error = ''
    this.sourceHeight = src.height
    this.nativeSource = src
    this.step = 'preview'
  }

  /** Partage en cours : l'arrête puis rouvre le choix de la source. */
  async restartScreen() {
    await this.toggleVideo('screen')
    await this.pickScreen()
  }

  /** Retour au choix de la source (liste maison) ou nouveau passage par le sélecteur du système. */
  changeSource() {
    if (platform.capture && this.nativeSource) { this.dropPreview(); this.step = 'source' }
    else void this.pickScreen()
  }

  private stopNative() {
    if (this.nativeShare) {
      this.nativeShare = false
      void platform.capture?.stop()
    }
  }

  private dropPreview() {
    this.nativeSource = null
    this.preview?.getTracks().forEach((t) => t.stop())
    this.preview = null
  }

  closePicker() {
    this.dropPreview()
    this.picker = null
  }

  /** Résolution ou images/s changées dans la fenêtre d'aperçu : appliquées à la capture pour que l'aperçu les reflète. */
  async tunePreview() {
    const v = this.preview?.getVideoTracks()[0]
    if (!v) return
    const { res, fps } = clampScreen(prefs.screenRes, prefs.screenFps, this.maxScreen)
    const q = SCREEN_RES[res]
    await v.applyConstraints({ frameRate: { ideal: fps }, width: { ideal: q.width }, height: { ideal: q.height } }).catch(() => {})
  }

  /** Publie le flux de l'aperçu comme partage d'écran (et son son, s'il a été accordé). */
  async publishScreen() {
    const lp = this.room?.localParticipant
    if (lp && this.nativeSource) return this.publishNative(this.nativeSource)
    const s = this.preview
    if (!lp || !s) return
    this.preview = null // le flux appartient désormais à la publication
    this.picker = null
    const { res, fps } = clampScreen(prefs.screenRes, prefs.screenFps, this.maxScreen)
    debug.log('info', 'partage', `publication ${res} ${fps} i/s`)
    try {
      const { LocalVideoTrack, LocalAudioTrack, Track } = await import('livekit-client')
      await lp.publishTrack(new LocalVideoTrack(s.getVideoTracks()[0], undefined, true), {
        source: Track.Source.ScreenShare,
        screenShareEncoding: { maxBitrate: screenBitrate(res, fps), maxFramerate: fps },
        degradationPreference: 'maintain-resolution',
        // Au-delà de 1080p, des couches réduites (simulcast) évitent d'imposer le flux entier aux spectateurs à faible débit.
        simulcast: SCREEN_RES[res].height > 1080,
      })
      const a = s.getAudioTracks()[0]
      if (a) await lp.publishTrack(new LocalAudioTrack(a, undefined, true), { source: Track.Source.ScreenShareAudio })
      this.screen = true
      this.watchScreen({ res, fps })
      await this.applyOverlay()
    } catch (e) {
      debug.log('error', 'partage', `publication impossible : ${e instanceof Error ? e.message : String(e)}`)
      this.stopNative()
      s.getTracks().forEach((t) => t.stop())
      this.error = "Partage d'écran impossible."
    }
    this.syncTiles()
  }

  /** Alpha (EST-18) : Rust capture et publie la source choisie (second participant `#screen`). Pas de repli sur le webview. */
  private async publishNative(src: CaptureSource) {
    const { res, fps } = clampScreen(prefs.screenRes, prefs.screenFps, this.maxScreen)
    debug.log('info', 'partage natif', `source ${src.id} (${src.kind}), ${res} ${fps} i/s`)
    try {
      if (!this.channelId) throw new Error('aucun salon vocal rejoint')
      const { url, token } = await api.voiceScreenToken(this.channelId)
      // Un serveur sans cette route renvoie un jeton normal : Rust se connecterait avec notre identité et LiveKit nous éjecterait du salon.
      const sub = JSON.parse(atob(token.split('.')[1].replace(/-/g, '+').replace(/_/g, '/'))).sub as string
      if (!sub.endsWith('#screen')) throw new Error("le serveur ne gère pas encore le partage natif (jeton sans `#screen`), il faut déployer le backend")
      notify.info(`Alpha : encodeurs disponibles ${await platform.capture!.start(url, token, src.id, fps, SCREEN_RES[res].height, screenBitrate(res, fps), prefs.screenAudio)}`)
    } catch (e) {
      debug.log('error', 'partage natif', `échec : ${e instanceof Error ? e.message : String(e)}`)
      this.error = `Partage d'écran natif impossible : ${e instanceof Error ? e.message : String(e)}`
      return
    }
    this.closePicker()
    this.nativeShare = true
    this.screen = true
    this.syncTiles()
  }

  /** Qualité ou codec de la caméra modifiés pendant un appel : la caméra est relancée avec les nouveaux réglages. */
  async restartCamera() {
    const lp = this.room?.localParticipant
    if (!lp || !this.camera) return
    try {
      const o = cameraOptions()
      await lp.setCameraEnabled(false)
      await lp.setCameraEnabled(true, o.capture, o.publish)
      await this.applyFx()
    } catch {
      this.camera = false
      this.error = 'Caméra indisponible.'
    }
    this.syncTiles()
  }

  /** Arrière-plan de la caméra publiée mis en accord avec les réglages (flou, fond, image) ; « Aucun » ne charge rien. */
  async applyFx() {
    const track = this.room?.localParticipant.getTrackPublication('camera' as never)?.videoTrack as LocalVideoTrack | undefined
    const ok = await applyCameraFx(track, () => {
      prefs.cameraFx = 'none'
      prefs.save()
      void this.applyFx()
      this.error = "Effet d'arrière-plan désactivé : l'ordinateur ne suit pas."
    })
    if (!ok) this.error = "Effet d'arrière-plan indisponible sur cet appareil."
    await this.applyOverlay()
  }

  private stopOverlay?: () => Promise<void>
  private overlayJob: Promise<void> = Promise.resolve()

  /** Incrustation de la caméra dans le partage (EST-150, Alpha) en accord avec les réglages ; désactivée, rien n'est chargé. */
  applyOverlay() {
    return (this.overlayJob = this.overlayJob.then(async () => {
      if (this.stopOverlay) {
        await this.stopOverlay()
        this.stopOverlay = undefined
        void this.room?.localParticipant.setAttributes({ overlay: '' })
      }
      const lp = this.room?.localParticipant
      const screen = this.localScreen()
      const cam = lp?.getTrackPublication('camera' as never)?.videoTrack as LocalVideoTrack | undefined
      if (!prefs.camOverlay.enabled || !screen || !cam || this.nativeShare) return
      try {
        this.stopOverlay = await (await import('../../lib/camoverlay')).startOverlay(screen, cam, (m) => { this.error = m })
        void lp?.setAttributes({ overlay: '1' })
      } catch (e) {
        debug.log('error', 'incrustation', `impossible : ${e instanceof Error ? e.message : String(e)}`)
        this.error = "Incrustation de la caméra indisponible sur cet appareil."
      }
      this.syncTiles()
    }))
  }

  private async toggleVideo(kind: 'camera' | 'screen') {
    const lp = this.room?.localParticipant
    if (!lp) return
    const on = kind === 'camera' && !this.camera
    this.error = ''
    try {
      if (kind === 'camera') {
        const o = cameraOptions()
        await lp.setCameraEnabled(on, o.capture, o.publish)
        if (on) await this.applyFx()
        else await this.applyOverlay()
      }
      else await lp.setScreenShareEnabled(false) // le partage démarre par pickScreen puis publishScreen
      if (kind === 'camera') this.camera = on
      else {
        this.screen = false
        this.stopNative()
        this.sourceHeight = null
        this.stopAdapt()
      }
    } catch (e) {
      // Annuler le sélecteur de fenêtre n'est pas une erreur à afficher.
      const denied = e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'AbortError')
      if (kind === 'screen' && denied) return
      this.error =
        denied ? (kind === 'camera' ? 'Accès à la caméra refusé.' : 'Partage d\'écran refusé.')
        : kind === 'camera' ? 'Caméra indisponible.'
        : 'Partage d\'écran impossible.'
    }
    this.syncTiles()
  }

  private stopAdapt() {
    clearInterval(this.adapt)
    this.adapt = undefined
  }

  private localScreen() {
    return this.room?.localParticipant.getTrackPublication('screen_share' as never)?.videoTrack as LocalVideoTrack | undefined
  }

  /** Applique un palier à la capture en cours (sans relancer le partage) : contraintes de la piste et débit maximal de l'encodeur. */
  private async applyScreen(track: LocalVideoTrack, c: { res: ScreenRes; fps: ScreenFps }) {
    const q = SCREEN_RES[c.res]
    await track.mediaStreamTrack
      .applyConstraints({ frameRate: { ideal: c.fps }, width: { ideal: q.width }, height: { ideal: q.height } })
      .catch(() => {})
    try {
      const params = track.sender?.getParameters()
      if (!params?.encodings?.length) return
      params.encodings.forEach((e) => (e.maxFramerate = c.fps))
      // En simulcast, seule la couche la plus haute (plus gros débit) prend le nouveau plafond.
      const top = params.encodings.reduce((a, b) => ((b.maxBitrate ?? 0) > (a.maxBitrate ?? 0) ? b : a))
      top.maxBitrate = screenBitrate(c.res, c.fps)
      await track.sender!.setParameters(params)
    } catch { /* l'encodeur garde ses plafonds : seule la capture a changé */ }
  }

  /** Qualité choisie en plein partage (prefs.screenRes / screenFps), imposée même si elle est plus haute que ce que la réduction automatique avait retenu. */
  async setScreenQuality() {
    const track = this.localScreen()
    if (!track) return
    const c = clampScreen(prefs.screenRes, prefs.screenFps, this.maxScreen)
    await this.applyScreen(track, c)
    this.watchScreen(c)
  }

  /**
   * Surveille le partage : si l'encodeur est freiné par le débit ou le processeur plusieurs mesures de suite,
   * la capture descend d'un palier (images/s puis résolution) ; on ne remonte pas seul, pour éviter les va-et-vient.
   * Sans effet sur la réduction si l'utilisateur l'a désactivée (prefs.screenAuto).
   */
  private watchScreen(start: { res: ScreenRes; fps: ScreenFps }) {
    this.stopAdapt()
    let cur = start
    let strikes = 0
    setTimeout(() => {
      const got = this.localScreen()?.mediaStreamTrack.getSettings().frameRate
      if (got && got < cur.fps * 0.85) notify.info(`Le système limite la capture à ${Math.round(got)} images/s.`)
    }, 2000)
    if (!prefs.screenAuto) return
    this.adapt = setInterval(async () => {
      const track = this.localScreen()
      if (!track) return
      const stats = await track.getSenderStats().catch(() => [])
      const limited = stats.some((s) => s.qualityLimitationReason === 'bandwidth' || s.qualityLimitationReason === 'cpu')
      strikes = limited ? strikes + 1 : 0
      if (strikes < 3) return
      strikes = 0
      const next = stepDown(cur)
      if (!next) return
      cur = next
      await this.applyScreen(track, next)
      notify.info(`Connexion ou ordinateur limités : partage réduit à ${next.res} · ${next.fps} images/s.`)
    }, 3000)
  }

  /** Bordure « parle » : mesurée sur le son réellement reçu ou émis plutôt que sur l'indicateur du serveur (en retard, et absent pour soi). */
  private watchVoice(sid: string, track: MediaStreamTrack, identity: string) {
    this.meters.get(sid)?.()
    const stop = watchSpeaking(track, (on) => {
      if (on) this.speaking[identity] = true
      else delete this.speaking[identity]
      this.syncStates()
    })
    this.meters.set(sid, () => { stop(); this.meters.delete(sid) })
  }

  /** Abonne la salle à ce qu'on veut recevoir : tout, sauf les partages d'écran qu'on ne regarde pas. */
  private syncSubs() {
    const room = this.room
    room?.remoteParticipants.forEach((p) => {
      p.trackPublications.forEach((pub) => {
        const want = !(pub.source === 'camera' && p.attributes.overlay) && shouldSubscribe(pub.source, this.watched[ownerOf(p.identity)], prefs.watchAuto, ownerOf(p.identity) === room.localParticipant.identity)
        if (want !== pub.isDesired) pub.setSubscribed(want)
      })
    })
    this.syncTiles()
  }

  watch(userId: string, on = true) {
    this.watched[userId] = on
    this.syncSubs()
  }

  /** Recalcule la liste des pistes vidéo à partir de l'état de la salle. */
  private syncTiles(gone?: LkTrack) {
    const room = this.room
    if (!room) return
    const tiles: VideoTile[] = []
    for (const p of [room.localParticipant, ...room.remoteParticipants.values()]) {
      for (const pub of p.videoTrackPublications.values()) {
        const screen = pub.source === 'screen_share'
        if (pub.source === 'camera' && (p.isLocal ? this.stopOverlay : p.attributes.overlay)) continue // incrustée dans le partage (EST-150) : pas de tuile en double
        const track = pub.track === gone ? undefined : pub.track // `gone` : piste en cours de désabonnement, encore rattachée à la publication
        if ((!track && !(screen && !p.isLocal)) || pub.isMuted) continue
        // le partage natif (EST-18) est publié par `<userId>#screen` : il s'affiche chez son propriétaire
        tiles.push({ sid: pub.trackSid, userId: p.identity.replace(/#screen$/, ''), name: p.name || p.identity, screen, track })
      }
    }
    this.tiles = tiles
  }

  async leave() {
    const room = this.room
    this.room = null
    if (room) {
      if (prefs.voiceSounds && this.state === 'connected') playTone('leave')
      debug.log('info', 'vocal', 'sortie du salon')
      await room.disconnect()
    }
    this.reset()
  }

  private reset() {
    if (this.channelId) this.dropped = { id: this.channelId, at: Date.now() }
    this.processor = null
    this.stopNative()
    this.closePicker() // un aperçu d'écran ouvert ne survit pas à la sortie du vocal
    this.stopAdapt()
    this.stopPoll()
    this.voiceActive = false
    this.quality = {}
    this.pinned = null
    this.offKeys?.()
    this.offKeys = null
    this.talking = false
    this.audio.forEach((el) => el.remove())
    this.audio = []
    this.room = null
    this.state = 'idle'
    this.channelId = null
    this.meters.forEach((stop) => stop())
    this.speaking = {}
    this.camera = false
    this.screen = false
    this.deafened = false
    this.states = {}
    this.watchers = {}
    this.watched = {}
    this.viewing.clear()
    this.watching = ''
    this.tiles = []
    this.syncCall()
    void this.applyOverlay()
  }
}

export const voice = new VoiceController()
