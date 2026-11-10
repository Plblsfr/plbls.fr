import { api } from '../../lib/api'
import { app } from '../../lib/store.svelte'
import { debug } from '../../lib/debuglog.svelte'
import { notify } from '../../lib/ui'
import { prefs } from '../../lib/prefs.svelte'
import { providers, type Adapter } from './providers'
import { playerError } from './watch'
import { SyncEngine, type Cmd } from './watchsync'
import type { SessionKind } from '../../lib/types'
import type { FilmTracks, Quality } from '../../sections/entracte/films'

// Chaque client lit la vidéo lui-même avec le lecteur de sa source (lib/providers.ts) : le serveur ne synchronise que l'état.
// Ce fichier branche le moteur (lib/watchsync.ts) sur le lecteur, l'état du serveur et l'API.

export const watchView = $state<{ player: WatchPlayer | null }>({ player: null })

export class WatchPlayer {
  error = $state('')
  /** Le navigateur refuse la lecture automatique : un clic est nécessaire. */
  blocked = $state(false)
  /** Coupe-circuit déclenché : la synchronisation reprend seule après quelques secondes. */
  suspended = $state(false)
  slow = $state(false)
  position = $state(0)
  duration = $state(0)
  tracks = $state<FilmTracks | null>(null)
  private p: Adapter | null = null
  private engine: SyncEngine | null = null
  private prov = ''
  private video = ''
  private slot: HTMLElement | null = null
  private timer: ReturnType<typeof setInterval> | undefined
  private gone = false
  private gen = 0

  constructor(private el: HTMLElement, private cid: string, private kind: SessionKind = 'watch') {}

  private get cur() { return app[this.kind][this.cid] }
  private get tag() { return this.kind === 'music' ? 'musique' : this.kind === 'cinema' ? 'cinema' : 'regarder' }
  get canDrive() { const s = this.cur?.state; return !!s && (!s.host_only || s.host === app.meId) }

  start() {
    this.timer = setInterval(() => this.tick(), 500)
  }

  /** Crée le lecteur de la source (le précédent est retiré : une file peut mêler plusieurs sources). */
  private async use(prov: string) {
    const gen = ++this.gen
    this.prov = prov
    this.slow = false
    this.tracks = null
    this.engine?.destroy()
    this.engine = null
    this.p?.destroy()
    this.slot?.remove()
    this.p = null
    const factory = providers[prov]
    if (!factory) { this.error = 'Cette source de vidéo n’est pas prise en charge par cette version.'; return }
    this.error = ''
    this.slot = this.el.appendChild(document.createElement('div'))
    this.slot.style.cssText = 'position:relative;width:100%;height:100%'
    const inner = this.slot.appendChild(document.createElement('div'))
    try {
      const a = await factory(inner, {
        onTracks: (t) => { this.tracks = t },
        onState: (st) => { debug.log('info', this.tag, `lecteur : ${st}`); this.engine?.observe(st) },
        onError: (m) => { this.error = m; debug.log('error', this.tag, `erreur du lecteur : ${m}`); if (!this.skipUnplayable()) void api.watchReady(this.cid, false, this.kind).catch(() => {}) },
      })
      if (this.gone || gen !== this.gen) { a.destroy(); return }
      if (a.sync === 'full') this.slot.appendChild(document.createElement('div')).style.cssText = 'position:absolute;inset:0' // les clics dans la vidéo ne commandent rien : tout passe par les contrôles d'Estompe
      this.p = a
      if (this.kind === 'music') a.setVolume?.(prefs.musicVolume)
      this.engine = new SyncEngine(a, {
        server: () => this.cur,
        send: (action, b) => api.watchAction(this.cid, action, b, this.kind),
        ended: (rev) => void api.watchAction(this.cid, 'ended', { rev }, this.kind).catch(() => {}),
        ready: (ready) => void api.watchReady(this.cid, ready, this.kind).catch(() => {}),
        refused: (e) => notify.error(e ?? new Error('Le serveur n’a pas répondu'), 'Action refusée'),
        log: (m) => debug.log('info', this.tag, m),
      })
      this.apply()
    } catch (e) {
      if (gen !== this.gen) return
      this.error = e instanceof Error ? e.message : 'Lecteur indisponible'
    }
  }

  destroy() {
    this.gone = true
    clearInterval(this.timer)
    this.engine?.destroy()
    this.p?.destroy()
  }

  /** Appelé à chaque nouvel état reçu : le lecteur de la source est créé si elle change, le reste revient au moteur. */
  apply() {
    const c = this.cur
    if (!c) return
    const prov = c.state.provider || 'youtube'
    if (c.state.video !== this.video) { this.video = c.state.video; if (prov === this.prov) this.error = '' } // une erreur ne concerne que la vidéo qui l'a causée
    if (prov !== this.prov) { void this.use(prov); return }
    this.engine?.receive()
    this.sync()
  }

  private sync() {
    const e = this.engine
    if (!e) return
    this.suspended = e.suspended
    this.blocked = e.stalled
    this.slow = e.slow
    this.position = e.position
  }

  private tick() {
    if (!this.engine || this.error) return
    this.engine.tick()
    this.duration = this.p?.duration?.() ?? 0
    this.sync()
  }

  /** Action de la personne sur un contrôle d'Estompe. */
  command(cmd: Cmd) {
    if (!this.canDrive) { notify.error(new Error('Seul l’hôte pilote la lecture'), 'Action refusée'); return }
    this.engine?.command(cmd)
    this.sync()
  }

  /** Volume de la personne (0 à 100), indépendant du vocal. */
  setVolume(v: number) { this.p?.setVolume?.(v) }

  setAudio(n: number) { this.p?.setAudio?.(n) }
  setSubtitle(n: number | null) { this.p?.setSubtitle?.(n); if (this.tracks) this.tracks = { ...this.tracks, subN: n } }
  /** Palier propre à ce participant (Entracte) : seule la position de la séance est partagée. */
  setQuality(q: Quality) { this.p?.setQuality?.(q) }

  /** Un morceau de Music2gether que le lecteur refuse (intégration désactivée, région, âge) est passé : le premier client à le signaler avance la file. */
  private skipUnplayable(): boolean {
    const s = this.cur?.state
    if (this.kind !== 'music' || !s?.meta || this.error === playerError(153)) return false // 153 : le problème est ici, pas dans le morceau, passer au suivant n'y changerait rien
    notify.info(`« ${s.meta.title} » ne peut pas être lu ici : passage au suivant.`)
    debug.log('info', this.tag, `morceau non lisible ${s.video} : ${this.error}`)
    setTimeout(() => void api.watchAction(this.cid, 'ended', { rev: s.rev }, this.kind).catch(() => {}), 1000) // une file de morceaux illisibles ne doit pas saturer la limite de débit
    return true
  }

  reload() {
    if (this.gone) return
    debug.log('info', this.tag, 'lecteur rechargé')
    this.error = ''
    this.blocked = false
    this.prov = ''
    this.video = ''
    this.apply()
  }

  /** Rend la main au lecteur après un blocage de lecture automatique. */
  resume() {
    this.blocked = false
    this.p?.play()
  }
}
