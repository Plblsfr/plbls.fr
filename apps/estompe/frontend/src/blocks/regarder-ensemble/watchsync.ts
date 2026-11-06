import type { Adapter, PState } from './providers'
import type { WatchState } from '../../lib/types'
import { correction, expectedPosition } from './watch'

export type Phase = 'loading' | 'synced' | 'pending' | 'buffering' | 'error'
export type Cmd = { action: 'play' | 'pause' | 'seek' | 'rate'; position?: number; rate?: number }

export interface Io {
  /** État confirmé par le serveur (jamais celui du lecteur). */
  server(): { state: WatchState; offset: number } | undefined
  send(action: string, b: { position?: number; rate?: number; video: string }): Promise<unknown>
  ended(rev: number): void
  ready(ready: boolean): void
  /** Commande refusée ou perdue : le lecteur est recalé sur le serveur. */
  refused(e?: unknown): void
  log(msg: string): void
}

const PENDING_MS = 3000 // une commande sans réponse du serveur est abandonnée
const BREAKER_N = 20 // évènements du serveur par seconde au-delà desquels la session est suspendue
const BREAKER_PAUSE_MS = 5000
const SEEK_TOLERANCE = 0.5

/**
 * Machine à états de la synchronisation d'un lecteur. Le serveur fait foi : l'état reçu est appliqué au lecteur sans jamais produire de commande ;
 * seules les actions de la personne (`command`) en produisent, une à la fois, la suivante attendant la confirmation de la précédente.
 * Les évènements du lecteur (`observe`) ne servent qu'à connaître son état.
 */
export class SyncEngine {
  phase: Phase = 'loading'
  /** Position du lecteur (secondes), mise à jour par `tick`. */
  position = 0
  /** Le lecteur n'a pas démarré alors que la session est en lecture (lecture automatique refusée). */
  stalled = false
  private video = ''
  private repeatRev = -1
  private base: number | null = null // révision sur laquelle s'appuie la commande en vol
  private queue: Cmd[] = []
  private pendTimer: ReturnType<typeof setTimeout> | undefined
  private nudgeTimer: ReturnType<typeof setTimeout> | undefined
  private pauseTimer: ReturnType<typeof setTimeout> | undefined
  private stamps: number[] = []
  private waitingSince = 0
  private key = ''
  private sentReady: boolean | null = null

  constructor(private p: Adapter, private io: Io) {}

  private get cur() { return this.io.server() }
  private expected() { const c = this.cur; return c ? expectedPosition(c.state, c.offset) : 0 }

  destroy() {
    clearTimeout(this.pendTimer)
    clearTimeout(this.nudgeTimer)
    clearTimeout(this.pauseTimer)
  }

  /** Nouvel état du serveur (ou premier appel) : appliqué au lecteur. */
  receive() {
    const c = this.cur
    if (!c || this.phase === 'error') return
    const s = c.state
    const key = `${s.rev}|${s.at}`
    if (key === this.key) return // même état de lecture (seuls les indicateurs « prêt » ont changé)
    this.key = key
    const now = Date.now()
    this.stamps = [...this.stamps.filter((t) => now - t < 1000), now]
    if (this.stamps.length > BREAKER_N) { this.trip(); return }
    if (this.base !== null) {
      if (s.rev <= this.base) return // plus ancien que notre dernière commande
      this.base = null
      clearTimeout(this.pendTimer)
      if (this.phase === 'pending') this.phase = 'synced'
    }
    this.applyServer()
    this.flush()
  }

  get suspended() { return this.phase === 'error' }

  private trip() {
    this.io.log('coupe-circuit : trop d’évènements, session suspendue')
    this.phase = 'error'
    this.queue = []
    this.base = null
    this.destroy()
    this.pauseTimer = setTimeout(() => { this.stamps = []; this.phase = 'synced'; this.receive() }, BREAKER_PAUSE_MS)
  }

  /** Charge la vidéo si elle change, sinon aligne lecture, position et vitesse sur le serveur. */
  private applyServer() {
    const c = this.cur!
    const s = c.state
    const again = s.act === 'repeat' && s.rev !== this.repeatRev // même morceau relu depuis le début
    if (s.video !== this.video || again) {
      this.repeatRev = s.rev
      this.video = s.video
      this.sentReady = null
      this.stalled = false
      this.waitingSince = 0
      this.phase = 'loading'
      clearTimeout(this.nudgeTimer)
      this.p.load(s.video, s.playing ? this.expected() : s.pos, s.playing)
      return
    }
    if (this.phase === 'loading' || this.p.sync === 'start') return
    const st = this.p.state()
    const exp = this.expected()
    if (s.playing) {
      if (st !== 'playing' && st !== 'buffering') this.p.play()
      if (Math.abs(this.p.time() - exp) >= 1) this.p.seek(exp)
    } else {
      if (st === 'playing' || st === 'buffering') this.p.pause()
      if (Math.abs(this.p.time() - s.pos) > SEEK_TOLERANCE) this.p.seek(s.pos)
    }
    if (this.p.canRate && this.p.rate() !== s.rate) this.p.setRate(s.rate)
    this.position = s.playing ? exp : s.pos
  }

  /** Action de la personne sur un contrôle d'Estompe : appliquée tout de suite au lecteur, envoyée au serveur dès que la précédente est confirmée. */
  command(cmd: Cmd) {
    if (this.phase === 'error' || this.phase === 'loading' || !this.cur) return
    switch (cmd.action) {
      case 'play': this.p.play(); break
      case 'pause': this.p.pause(); break
      case 'seek': this.p.seek(cmd.position!); this.position = cmd.position!; break
      case 'rate': this.p.setRate(cmd.rate!); break
    }
    // play/pause portent la position : ils remplacent un déplacement et l'autre play/pause en attente ; seule la dernière valeur de chaque réglage compte
    const pp = cmd.action === 'play' || cmd.action === 'pause'
    this.queue = this.queue.filter((q) => q.action !== cmd.action && !(pp && (q.action === 'seek' || q.action === 'play' || q.action === 'pause')))
    this.queue.push(cmd)
    this.flush()
  }

  private flush() {
    const c = this.cur
    if (this.base !== null || !c || !this.queue.length) return
    const cmd = this.queue.shift()!
    this.base = c.state.rev
    this.phase = 'pending'
    this.io.log(`commande ${cmd.action} (rév ${this.base})`)
    this.pendTimer = setTimeout(() => this.fail(), PENDING_MS)
    this.io.send(cmd.action, { position: cmd.position, rate: cmd.rate, video: this.video }).catch((e) => this.fail(e))
  }

  private fail(e?: unknown) {
    clearTimeout(this.pendTimer)
    this.base = null
    this.queue = []
    this.phase = 'synced'
    this.io.refused(e)
    if (this.cur) this.applyServer()
  }

  /** État du lecteur observé (chargement, fin, tampon) : jamais de commande. */
  observe(st: PState) {
    if (st === 'buffering') this.report(false)
    else if (st === 'playing' || st === 'paused' || st === 'cued') this.report(true)
    if (st === 'playing') { this.stalled = false; this.waitingSince = 0 }
    if (this.phase === 'error') return
    if (st === 'ended') { const c = this.cur; if (c) this.io.ended(c.state.rev); return }
    if (this.phase === 'loading' && (st === 'playing' || st === 'paused' || st === 'cued')) this.phase = this.base === null ? 'synced' : 'pending'
    else if (this.phase === 'synced' && st === 'buffering') this.phase = 'buffering'
    else if (this.phase === 'buffering' && st !== 'buffering') this.phase = 'synced'
  }

  private report(ready: boolean) {
    if (this.sentReady === ready) return
    this.sentReady = ready
    this.io.ready(ready)
  }

  /** Appelé régulièrement : position affichée et recalage de la lecture (jamais en pause, jamais pendant une commande, aucun `play`). */
  tick() {
    const c = this.cur
    if (!c || this.phase === 'loading' || this.phase === 'error' || this.p.sync === 'start') return
    const s = c.state
    const st = this.p.state()
    const pos = this.p.time()
    this.position = pos
    if (!s.playing || this.base !== null || this.queue.length) { this.waitingSince = 0; return }
    if (st !== 'playing') {
      if (st !== 'buffering') {
        this.waitingSince ||= Date.now()
        if (Date.now() - this.waitingSince > 3000) this.stalled = true
      }
      return
    }
    if (this.nudgeTimer) return // une correction de vitesse est déjà en cours
    if (this.p.canRate && this.p.rate() !== s.rate) { this.p.setRate(s.rate); return }
    const exp = this.expected()
    const drift = pos - exp
    const fix = correction(drift, exp, s.rate)
    if (!fix) return
    if (!('seek' in fix) && !this.p.canRate) return // pas de réglage de vitesse : seul un grand écart est rattrapé
    if (Math.abs(drift) >= SEEK_TOLERANCE) this.io.log(`écart ${drift.toFixed(2)} s → ${'seek' in fix ? 'saut' : 'vitesse ' + fix.rate}`)
    if ('seek' in fix) { this.p.seek(fix.seek); return }
    this.p.setRate(fix.rate)
    this.nudgeTimer = setTimeout(() => { this.nudgeTimer = undefined; this.p.setRate(this.cur?.state.rate ?? 1) }, fix.ms)
  }
}
