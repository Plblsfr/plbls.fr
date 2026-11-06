import { setClientPlatform, setUpgradeHandler } from './api'
import { platform } from './platform'
import { voice } from '../blocks/vocal/voice.svelte'

const EVERY = 30 * 60_000
/** Délai laissé avant de bloquer l'écran d'un utilisateur en vocal ou en train d'écrire. */
export const GRACE_MS = 2 * 60_000

/** `a` est une version strictement plus récente que `b` (x.y.z). */
export function isNewer(a: string, b: string): boolean {
  const [x, y] = [a, b].map((v) => v.split('.').map(Number))
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0)
  return false
}

/** Contenu publié (latest.json, android.json, web.json) : `mandatory` impose cette release, `minVersion` oblige tout client plus ancien. `force` : ancien nom de `mandatory`. */
export interface Remote { version: string; notes?: string; mandatory?: boolean; force?: boolean; minVersion?: string; message?: string; sha256?: string }

export interface Available { version: string; notes: string; message: string; force: boolean; sha256: string }

/** Ce que doit proposer l'app : rien, une mise à jour qu'on peut reporter, ou une mise à jour imposée (release « mandatory » plus récente, ou version installée sous `minVersion`). */
export function availability(installed: string, r: Remote): Available | null {
  if (!installed || !r.version || !isNewer(r.version, installed)) return null
  const force = r.mandatory === true || r.force === true || (!!r.minVersion && isNewer(r.minVersion, installed))
  return { version: r.version, notes: r.notes ?? '', message: r.message ?? '', force, sha256: r.sha256 ?? '' }
}

/** Vrai si l'utilisateur est en vocal ou a le focus dans un champ de saisie non vide : on lui laisse le temps de finir avant de bloquer l'écran. */
export function userIsBusy(): boolean {
  if (voice.state !== 'idle') return true
  const el = document.activeElement as HTMLInputElement | null
  return !!el && (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT' || el.isContentEditable) && !!(el.value ?? el.textContent)
}

/** Mises à jour (chaque plateforme fournit `platform.updates`). Détection périodique, installation à la demande ; les mises à jour imposées bloquent l'écran (ForcedUpdate). */
class Updater {
  available = $state<Available | null>(null)
  /** Refus 426 de l'API : message du serveur, même si le JSON de version n'a pas encore été relu. */
  refused = $state<string | null>(null)
  checking = $state(false)
  installing = $state(false)
  /** Avancement du téléchargement, de 0 à 1 (0 si la taille est inconnue). */
  progress = $state(0)
  /** Dernière erreur d'une vérification manuelle ou d'une installation. */
  error = $state('')
  version = $state(__APP_VERSION__)
  /** Secondes avant le blocage (utilisateur occupé) ; 0 hors compte à rebours. */
  countdown = $state(0)
  /** L'écran « Mise à jour requise » est affiché. */
  blocked = $state(false)
  /** Version dont l'utilisateur a repoussé la proposition (« Plus tard »). */
  dismissed = $state('')
  private timer: ReturnType<typeof setInterval> | undefined

  get forced() { return this.available?.force === true || this.refused !== null }

  /** `manual` : l'utilisateur a demandé la vérification, les erreurs lui sont montrées. */
  async check(manual = false): Promise<void> {
    if (this.checking || this.installing) return
    this.checking = true
    if (manual) this.error = ''
    try {
      const r = await platform.updates.fetchRemote()
      this.available = r ? availability(this.version, r) : null
      if (this.forced) this.arm()
    } catch (e) {
      if (manual) this.error = String(e)
    } finally {
      this.checking = false
    }
  }

  /** Une mise à jour est imposée : blocage immédiat, ou après un compte à rebours si l'utilisateur est occupé. */
  private arm() {
    if (this.blocked || this.timer) return
    if (!userIsBusy()) { this.block(); return }
    this.countdown = GRACE_MS / 1000
    this.timer = setInterval(() => { if (--this.countdown <= 0) this.block() }, 1000)
  }

  private block() {
    clearInterval(this.timer)
    this.timer = undefined
    this.countdown = 0
    this.blocked = true
    if (platform.updates.installOnBlock) void this.install() // web : rechargement automatique, rien d'autre à faire
  }

  async install(): Promise<void> {
    if (this.installing) return
    this.installing = true
    this.progress = 0
    this.error = ''
    try {
      if (await platform.updates.install(this.available, (p) => (this.progress = p))) this.installing = false
    } catch (e) {
      this.error = String(e)
      this.installing = false
    }
  }

  /** L'API a refusé la version de l'app (426) : même écran qu'une mise à jour imposée. */
  require(message: string) {
    this.refused = message
    this.arm()
    void this.check()
  }

  /** À appeler une fois au démarrage : déclare version et plateforme à l'API, vérifie après un court délai, au retour au premier plan, puis toutes les 30 minutes. */
  start() {
    setClientPlatform(platform.id())
    setUpgradeHandler((m) => this.require(m))
    void platform.updates.version?.().then((v) => (this.version = v)).catch(() => {})
    setTimeout(() => void this.check(), 15_000)
    setInterval(() => void this.check(), EVERY)
    document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') void this.check() })
  }
}

export const updater = new Updater()
