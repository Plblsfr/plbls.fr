/** Réglages de l'overlay de l'app Windows (pastille des participants, messages, Bobyn). */
export type OverlayCorner = 'tl' | 'ml' | 'bl' | 'tr' | 'mr' | 'br'
export type OverlayWhen = 'games' | 'always' | 'never'
export type OverlaySize = 'sm' | 'md' | 'lg'
export type OverlayMode = 'all' | 'speaking'
export type MsgOverlayKinds = 'both' | 'dm' | 'mention' | 'off'

export type Density = 'confortable' | 'compacte'
export type Theme = 'auto' | 'light' | 'dark'
export type NoiseMode = 'off' | 'standard' | 'strong'
export type ScreenRes = '720p' | '1080p' | '1440p' | '2160p'
export type ScreenFps = 30 | 60 | 120
/** Sélecteur du partage d'écran dans l'app Windows : celui du système (avec le son) ou celui d'Estompe (BETA, sans le son). */
export type CameraQuality = '360p' | '720p' | '1080p'
export type CameraFx = 'none' | 'blur' | 'cutout' | 'image'
/** Incrustation de la caméra dans le partage d'écran (EST-150, Alpha) : position (0 à 1 dans l'espace libre), largeur (part de l'image), forme. */
export interface CamOverlay { enabled: boolean; x: number; y: number; size: number; shape: 'rect' | 'circle' | 'free'; margin: number; border: boolean; shadow: boolean; mirror: boolean }
export type VideoCodecPref = 'auto' | 'vp9' | 'h264'
export type SoundChoice = 'off' | 'rise' | 'fall' | 'chime' | 'pop' | 'double' | 'custom'

interface Stored {
  desktopNotifications: boolean; density: Density; showPreviews: boolean; theme: Theme
  /** Transcrire automatiquement mes messages vocaux (EST-158). */
  transcribeAuto: boolean
  micId: string; speakerId: string; cameraId: string; noise: NoiseMode
  micGain: number; screenAudio: boolean; ptt: boolean; pttKey: string; voiceSounds: boolean; screenRes: ScreenRes; screenFps: ScreenFps; screenAuto: boolean
  /** Regarder automatiquement les streams des autres (sinon : tuile « Regarder », flux non reçu). */
  watchAuto: boolean
  /** Vrai une fois l'option réglée explicitement : sinon la valeur enregistrée est ignorée au profit du défaut (EST-81). */
  watchAutoSet: boolean
  /** Détection de voix : le micro n'émet que si le niveau dépasse le seuil (en dBFS). Sans effet en push-to-talk. */
  vad: boolean; vadThreshold: number
  cameraQuality: CameraQuality; videoCodec: VideoCodecPref
  /** Arrière-plan de la caméra (EST-151) : effet, intensité du flou, fond choisi (identifiant d'un fond proposé ou « custom »), image perso (data: URI, propre à cet appareil). */
  cameraFx: CameraFx; cameraBlur: number; cameraBg: string; customBg: string
  /** Incrustation de la caméra dans le partage d'écran (EST-150) ; désactivée par défaut. */
  camOverlay: CamOverlay
  joinSound: SoundChoice; leaveSound: SoundChoice; soundVolume: number
  /** Sons personnalisés (data: URI, 150 Ko au plus), propres à cet appareil. */
  customJoin: string; customLeave: string
  /** Volume d'écoute par utilisateur, de 0 à 300 % (100 par défaut). */
  volumes: Record<string, number>
  /** Volume de Music2gether (0 à 100), indépendant du vocal. */
  musicVolume: number
  /** Idem pour le son du partage d'écran, et sourdines (voix / partage) par utilisateur. */
  screenVolumes: Record<string, number>; mutedVoice: Record<string, boolean>; mutedScreen: Record<string, boolean>
  /** Dernier onglet des paramètres ouvert. */
  settingsTab: string
  /** Permission système déjà demandée une fois. */
  notifAsked: boolean
  /** Overlay des participants du salon vocal (app Windows). */
  overlayWhen: OverlayWhen; overlayKey: string; overlayCorner: OverlayCorner; overlaySize: OverlaySize; overlayMode: OverlayMode
  /** Messages privés et mentions dans l'overlay (app Windows) : quoi afficher, contenu masqué, durée en secondes, raccourcis globaux « répondre » et « ouvrir ». */
  msgOverlay: MsgOverlayKinds; msgHide: boolean; msgSeconds: number; msgReplyKey: string; msgOpenKey: string
  /** Pastille Bobyn sur le jeu enregistré (app Windows) : coin, ou 'off'. */
  bobynOverlay: OverlayCorner | 'off'
  /** Partager le jeu en cours avec les personnes côtoyées (app Windows) ; désactivé par défaut. */
  shareActivity: boolean
  /** Montrer le morceau écouté seul dans l'activité (🎵 Écoute …) ; désactivé par défaut. */
  shareListening: boolean
  /** Android : lecteur 2Gether en image dans l'image quand on quitte l'app (EST-179). */
  floatingPlayer: boolean
  /** Mode debug (EST-48) : journal détaillé, voir debuglog. */
  debug: boolean
  /** Fonctions alpha débloquées (onglet Bobyn) : réglage local, masqué. */
  alpha: boolean
  /** Ordre des serveurs dans la barre de gauche (identifiants), choisi par glisser-déposer. */
  serverOrder: string[]
}
const KEY = 'estompe.prefs'
const DEFAULTS: Stored = { desktopNotifications: true, density: 'confortable', showPreviews: true, theme: 'auto', transcribeAuto: true,
  micId: '', speakerId: '', cameraId: '', noise: 'strong', micGain: 100, screenAudio: true, ptt: false, pttKey: 'KeyV', voiceSounds: true, screenRes: '1080p', screenFps: 30, screenAuto: true, watchAuto: true, watchAutoSet: false,
  vad: false, vadThreshold: -45, cameraQuality: '720p', videoCodec: 'auto', cameraFx: 'none', cameraBlur: 10, cameraBg: 'aube', customBg: '',
  camOverlay: { enabled: false, x: 1, y: 1, size: 0.22, shape: 'rect', margin: 24, border: false, shadow: true, mirror: false },
  joinSound: 'rise', leaveSound: 'fall', soundVolume: 60, customJoin: '', customLeave: '', volumes: {}, musicVolume: 80, screenVolumes: {}, mutedVoice: {}, mutedScreen: {}, settingsTab: 'account', notifAsked: false,
  overlayWhen: 'games', overlayKey: 'Control+Shift+KeyO', overlayCorner: 'ml', overlaySize: 'md', overlayMode: 'all', msgOverlay: 'both', msgHide: false, msgSeconds: 8, msgReplyKey: 'Control+Shift+KeyR', msgOpenKey: 'Control+Shift+KeyE', bobynOverlay: 'tr', shareActivity: false, shareListening: false, floatingPlayer: true, debug: false, alpha: false, serverOrder: [],
}

function load(): Stored {
  try {
    const raw = localStorage.getItem(KEY)
    if (raw) {
      const p = JSON.parse(raw)
      const old = /^(\d+p)(\d+)$/.exec(p.screenQuality ?? '') // ancien réglage « 1080p60 » : résolution et images/s séparées
      if (old && !p.screenRes) Object.assign(p, { screenRes: old[1], screenFps: Number(old[2]) })
      if (p.overlay === false && !p.overlayWhen) p.overlayWhen = 'never' // ancien interrupteur de l'overlay
      if (!p.watchAutoSet) delete p.watchAuto
      return { ...DEFAULTS, ...p, camOverlay: { ...DEFAULTS.camOverlay, ...p.camOverlay } }
    }
  } catch { /* stockage indisponible : valeurs par défaut */ }
  return { ...DEFAULTS }
}

class Prefs {
  desktopNotifications = $state(DEFAULTS.desktopNotifications)
  density = $state<Density>(DEFAULTS.density)
  showPreviews = $state(DEFAULTS.showPreviews)
  transcribeAuto = $state(DEFAULTS.transcribeAuto)
  theme = $state<Theme>(DEFAULTS.theme)
  micId = $state(DEFAULTS.micId)
  speakerId = $state(DEFAULTS.speakerId)
  cameraId = $state(DEFAULTS.cameraId)
  noise = $state<NoiseMode>(DEFAULTS.noise)
  micGain = $state(DEFAULTS.micGain)
  screenAudio = $state(DEFAULTS.screenAudio)
  ptt = $state(DEFAULTS.ptt)
  pttKey = $state(DEFAULTS.pttKey)
  voiceSounds = $state(DEFAULTS.voiceSounds)
  screenRes = $state<ScreenRes>(DEFAULTS.screenRes)
  screenFps = $state<ScreenFps>(DEFAULTS.screenFps)
  screenAuto = $state(DEFAULTS.screenAuto)
  watchAuto = $state(DEFAULTS.watchAuto)
  watchAutoSet = $state(DEFAULTS.watchAutoSet)
  vad = $state(DEFAULTS.vad)
  vadThreshold = $state(DEFAULTS.vadThreshold)
  cameraQuality = $state<CameraQuality>(DEFAULTS.cameraQuality)
  videoCodec = $state<VideoCodecPref>(DEFAULTS.videoCodec)
  cameraFx = $state<CameraFx>(DEFAULTS.cameraFx)
  cameraBlur = $state(DEFAULTS.cameraBlur)
  cameraBg = $state(DEFAULTS.cameraBg)
  customBg = $state(DEFAULTS.customBg)
  camOverlay = $state<CamOverlay>({ ...DEFAULTS.camOverlay })
  joinSound = $state<SoundChoice>(DEFAULTS.joinSound)
  leaveSound = $state<SoundChoice>(DEFAULTS.leaveSound)
  soundVolume = $state(DEFAULTS.soundVolume)
  customJoin = $state(DEFAULTS.customJoin)
  customLeave = $state(DEFAULTS.customLeave)
  volumes = $state<Record<string, number>>({})
  musicVolume = $state(DEFAULTS.musicVolume)
  screenVolumes = $state<Record<string, number>>({})
  mutedVoice = $state<Record<string, boolean>>({})
  mutedScreen = $state<Record<string, boolean>>({})
  settingsTab = $state(DEFAULTS.settingsTab)
  notifAsked = $state(DEFAULTS.notifAsked)
  overlayWhen = $state<OverlayWhen>(DEFAULTS.overlayWhen)
  overlayKey = $state(DEFAULTS.overlayKey)
  overlayCorner = $state<OverlayCorner>(DEFAULTS.overlayCorner)
  overlaySize = $state<OverlaySize>(DEFAULTS.overlaySize)
  overlayMode = $state<OverlayMode>(DEFAULTS.overlayMode)
  msgOverlay = $state<MsgOverlayKinds>(DEFAULTS.msgOverlay)
  msgHide = $state(DEFAULTS.msgHide)
  msgSeconds = $state(DEFAULTS.msgSeconds)
  msgReplyKey = $state(DEFAULTS.msgReplyKey)
  msgOpenKey = $state(DEFAULTS.msgOpenKey)
  bobynOverlay = $state<OverlayCorner | 'off'>(DEFAULTS.bobynOverlay)
  shareActivity = $state(DEFAULTS.shareActivity)
  shareListening = $state(DEFAULTS.shareListening)
  floatingPlayer = $state(DEFAULTS.floatingPlayer)
  debug = $state(DEFAULTS.debug)
  alpha = $state(DEFAULTS.alpha)
  serverOrder = $state<string[]>(DEFAULTS.serverOrder)

  constructor() {
    Object.assign(this, load())
  }
  save() {
    const s: Stored = {
      desktopNotifications: this.desktopNotifications, density: this.density, showPreviews: this.showPreviews, theme: this.theme, transcribeAuto: this.transcribeAuto,
      micId: this.micId, speakerId: this.speakerId, cameraId: this.cameraId, noise: this.noise, micGain: this.micGain, screenAudio: this.screenAudio,
      ptt: this.ptt, pttKey: this.pttKey, voiceSounds: this.voiceSounds, screenRes: this.screenRes, screenFps: this.screenFps, screenAuto: this.screenAuto, watchAuto: this.watchAuto, watchAutoSet: this.watchAutoSet,
      vad: this.vad, vadThreshold: this.vadThreshold, cameraQuality: this.cameraQuality, videoCodec: this.videoCodec, cameraFx: this.cameraFx, cameraBlur: this.cameraBlur, cameraBg: this.cameraBg, customBg: this.customBg, camOverlay: $state.snapshot(this.camOverlay),
      joinSound: this.joinSound, leaveSound: this.leaveSound, soundVolume: this.soundVolume, customJoin: this.customJoin, customLeave: this.customLeave, volumes: $state.snapshot(this.volumes), musicVolume: this.musicVolume, screenVolumes: $state.snapshot(this.screenVolumes), mutedVoice: $state.snapshot(this.mutedVoice), mutedScreen: $state.snapshot(this.mutedScreen), settingsTab: this.settingsTab, notifAsked: this.notifAsked,
      overlayWhen: this.overlayWhen, overlayKey: this.overlayKey, overlayCorner: this.overlayCorner, overlaySize: this.overlaySize, overlayMode: this.overlayMode, msgOverlay: this.msgOverlay, msgHide: this.msgHide, msgSeconds: this.msgSeconds, msgReplyKey: this.msgReplyKey, msgOpenKey: this.msgOpenKey, bobynOverlay: this.bobynOverlay, shareActivity: this.shareActivity, shareListening: this.shareListening, floatingPlayer: this.floatingPlayer, debug: this.debug, alpha: this.alpha, serverOrder: $state.snapshot(this.serverOrder),
    }
    try { localStorage.setItem(KEY, JSON.stringify(s)) } catch { /* ignoré */ }
  }
}

export const prefs = new Prefs()
