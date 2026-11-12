/** Ce que chaque plateforme (web, windows, android) fournit au frontend : le frontend n'en teste aucune, il appelle `platform`. */
import type { Component } from 'svelte'
import type { IconName } from '../components/Icon.svelte'
import type { Available, Remote } from './updater.svelte'

export interface UpdateDriver {
  /** Version installée, si elle ne se lit pas dans le build. */
  version?(): Promise<string>
  fetchRemote(): Promise<Remote | null>
  /** Renvoie true si l'app reste ouverte après l'installation (sinon elle est relancée ou rechargée). */
  install(available: Available | null, progress: (p: number) => void): Promise<boolean>
  /** Mise à jour imposée : l'installer d'elle-même (web : rechargement) plutôt que d'attendre un clic. */
  installOnBlock: boolean
  installLabel: string
}

export interface AuthDriver {
  setup(): void
  login(register: boolean): Promise<void>
  diagnose(): Promise<string>
  logout(): Promise<void>
}

export interface CaptureSource { id: string; kind: 'screen' | 'window'; title: string; app: string; width: number; height: number }

/** Partage natif : liste des écrans et fenêtres, miniatures, capture et publication par le code natif. */
export interface CaptureDriver {
  listSources(): Promise<CaptureSource[]>
  /** Miniature sous forme d'URL locale (à libérer avec URL.revokeObjectURL). */
  thumbUrl(id: string): Promise<string>
  start(url: string, token: string, id: string, fps: number, height: number, bitrate: number, audio: boolean): Promise<string>
  stop(): Promise<void>
}

export interface PushDriver {
  supported(): boolean
  enabled(): Promise<boolean>
  /** Renvoie false si la permission est refusée ou si le serveur n'a pas le push. */
  enable(): Promise<boolean>
  disable(): Promise<void>
  /** Au démarrage : réenregistre l'abonnement. */
  refresh(): Promise<void>
  /** Le serveur peut répondre 501 (push absent) : le tester à l'ouverture des réglages. */
  probeServer: boolean
  /** Jeton de l'appareil (FCM sur Android), réutilisé par Entracte pour ses propres notifications. */
  token?(): Promise<string>
}

export interface NotificationDriver {
  /** Affiche une notification (`conv` : conversation à marquer lue par le bouton du toast ; `icon` : icône du serveur). */
  show(n: { title: string; body: string; href: string; conv: string; tag: string; icon: string }): Promise<void>
  supported(): boolean
  ask(): Promise<boolean>
  /** Le navigateur ignore la demande de permission sans geste de l'utilisateur. */
  needsGesture(): boolean
  push: PushDriver | null
  listenActions?(markRead: (conv: string) => void): void
  setBadge?(n: number): Promise<void>
  /** Bouton « notification de test » dans les réglages. */
  testable: boolean
  /** Indication affichée dans les réglages (ex. installer l'app sur iPhone). */
  hint?(): string
}

export interface GlobalPttDriver {
  register(code: string, onChange: (on: boolean) => void): Promise<() => Promise<void>>
  check(code: string): Promise<string>
}

export interface DebugStore {
  set(enabled: boolean): Promise<void>
  read(): Promise<string>
  write(line: string): void
  clear(): Promise<void>
}

/** Espace dédié d'une plateforme (ex. une bibliothèque de l'app) : entrée dans la barre des serveurs, menu latéral et page. */
export interface Space {
  id: string
  title: string
  icon: IconName
  visible(): boolean
  nav: Component
  page: Component<{ section?: string; onmenu: () => void }>
}

export interface Platform {
  /** Plateforme déclarée au serveur (tableau de bord d'administration). */
  id(): string
  updates: UpdateDriver
  /** Absent : connexion par le gateway (cookie de session). */
  auth?: AuthDriver
  notifications: NotificationDriver
  capture?: CaptureDriver
  globalPtt?: GlobalPttDriver
  debugStore?: DebugStore
  /** Capture de la fenêtre d'Estompe pour un signalement ; `auto` : sans accord préalable de l'utilisateur. */
  screenshot?: { auto: boolean; capture(): Promise<Blob | null> }
  spaces: Space[]
  /** Ce que la plateforme affiche dans les réglages et l'interface. */
  slots: {
    /** Page de connexion : liens de téléchargement des apps. */
    login?: Component
    /** Réglages › Application : mise à jour de l'app. */
    settingsUpdate?: Component
    settingsNotifications?: Component
    settingsVoice?: Component
    settingsPtt?: Component
    settingsShare?: Component
    settingsDebug?: Component
  }
  /** Démarrage de l'app principale (overlays, liens, activité…). */
  start(): void
  /** Fenêtres annexes (overlays) : `#hash` → composant ; page légère, sans session. */
  windows: Record<string, Component>
  /** Rouvre la dernière vue au lancement. */
  restoresLastView(): boolean
  /** Ouvre la conversation de la notification qui a lancé l'app. */
  openFromNotification(): boolean
  /** Retire la notification de cette conversation. */
  cancelNotification(conv: string): void
  /** Ctrl+Alt+B ouvre le signalement. */
  reportShortcut: boolean
  /** Ré-encode une vidéo en H.264 (HEVC et AV1 sont souvent illisibles dans un navigateur) ; absent, l'original part tel quel. */
  toH264?(file: Blob): Promise<Blob>
  /** Aide en cas de micro refusé par le système. */
  micHelp: string
  /** Libellé de la plateforme dans les journaux de debug. */
  label(): string
  /** Raccourci du push-to-talk : texte explicatif sous la touche. */
  pttNote: string
  /** Overlay des participants du vocal : interrupteur dans les contrôles vocaux. */
  voiceOverlay?: { available(): boolean; hidden(): boolean; toggle(): void }
  /** Vocal en cours : la plateforme le garde actif en arrière-plan (`channelId` null : plus de vocal). */
  voiceCall?: { update(s: { channelId: string | null; muted: boolean; deafened: boolean }): void }
  /** Application au premier plan (jeu en cours), si la plateforme sait la détecter. */
  foregroundApp?(): Promise<string | null>
}

let current: Platform | null = null

export function setPlatform(p: Platform) { current = p }

/** Faux tant que `main.ts` n'a pas installé la plateforme (les modules chargés par elle ne peuvent pas encore la lire). */
export const platformReady = () => current !== null

/** La plateforme du build ; `main.ts` (ou le test) l'installe avant tout autre module. */
export const platform: Platform = new Proxy({} as Platform, {
  get(_, k) {
    if (!current) throw new Error('plateforme non initialisée (setPlatform)')
    return current[k as keyof Platform]
  },
})
