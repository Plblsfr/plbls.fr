import { ApiError } from '../../lib/api'
import { medias } from '../../lib/medias'
import { notify } from '../../lib/ui'
import { debut, dureeExacte, gainLineaire, SILLON_URL, sillonApi, support, type DemandeLecture, type Lecture, type Piste, type Qualite, type Support } from './sillon'

export interface ElementAudio {
  src: string
  crossOrigin: string | null
  preload: string
  currentTime: number
  volume: number
  paused: boolean
  ended: boolean
  play(): Promise<void>
  pause(): void
  load(): void
  removeAttribute(nom: string): void
  canPlayType(type: string): string
  addEventListener(type: string, f: () => void): void
}

export interface NoeudGain { gain: { value: number }; connect(destination: unknown): unknown }

export interface ContexteAudio {
  state: string
  destination: unknown
  resume(): Promise<void>
  createMediaElementSource(el: never): { connect(destination: unknown): unknown }
  createGain(): NoeudGain
}

export interface DetailsAction { seekTime?: number | null; seekOffset?: number | null }

export interface SessionMedia {
  metadata: unknown
  playbackState: string
  setActionHandler(action: string, f: ((d: DetailsAction) => void) | null): void
  setPositionState?(s: { duration: number; position: number; playbackRate: number }): void
}

export interface Metadonnees { title: string; artist: string; album: string; artwork: { src: string; sizes: string }[] }

export interface Environnement {
  creerAudio(): ElementAudio
  contexteAudio?(): ContexteAudio | null
  ouvrir(id: string, demande: DemandeLecture): Promise<Lecture>
  base: string
  session?: SessionMedia | null
  metadonnees?(m: Metadonnees): unknown
  pochette?(album: string): Promise<string>
  stockage?: Pick<Storage, 'getItem' | 'setItem'> | null
}

export type Contexte = { type: 'album'; id: string } | { type: 'pistes' }

export interface Reprise { file: Piste[]; index: number; position: number; contexte: Contexte }

interface Emplacement { el: ElementAudio; piste: Piste | null; lecture: Lecture | null; gain: NoeudGain | null; jeton: number }

const CLE_REPRISE = 'sillon.reprise'
const CLE_VOLUME = 'sillon.volume'
const MAX_REPRISE = 200

const lireJson = <T>(s: Environnement['stockage'], cle: string): T | null => {
  try { const v = s?.getItem(cle); return v ? (JSON.parse(v) as T) : null } catch { return null }
}
const ecrire = (s: Environnement['stockage'], cle: string, v: unknown) => {
  try { s?.setItem(cle, JSON.stringify(v)) } catch { return }
}

export class Lecteur {
  file = $state.raw<Piste[]>([])
  index = $state(-1)
  contexte = $state.raw<Contexte>({ type: 'pistes' })
  lecture = $state(false)
  attente = $state(false)
  bloque = $state(false)
  position = $state(0)
  duree = $state(0)
  volume = $state(1)
  qualite = $state<Qualite | null>(null)
  erreur = $state('')
  reprise = $state.raw<Reprise | null>(null)

  private slots: [Emplacement, Emplacement] | null = null
  private actif = 0
  private ctx: ContexteAudio | null | undefined
  private formats: Support | null = null
  private tour = 0
  private actionsPosees = false
  private art: { album: string; url: string } | null = null

  constructor(private env: Environnement) {
    medias.ecouter((s) => { if (s !== 'sillon') this.pause() })
    const v = lireJson<number>(env.stockage, CLE_VOLUME)
    if (typeof v === 'number' && v >= 0 && v <= 1) this.volume = v
    this.reprise = lireJson<Reprise>(env.stockage, CLE_REPRISE)
  }

  get piste(): Piste | null { return this.file[this.index] ?? null }
  get aSuivre(): Piste[] { return this.file.slice(this.index + 1) }
  get elementActif(): ElementAudio | null { return this.slots?.[this.actif].el ?? null }
  get elementSuivant(): ElementAudio | null { return this.slots?.[1 - this.actif].el ?? null }

  jouer(pistes: Piste[], depart = 0, contexte: Contexte = { type: 'pistes' }) {
    const dispo = pistes.filter((p) => p.available)
    if (!dispo.length) return
    this.file = dispo
    this.contexte = contexte
    this.brancher()
    medias.demarre('sillon')
    void this.demarrer(Math.max(0, dispo.indexOf(pistes[depart])))
  }

  reprendreSauvegarde() {
    const r = this.reprise
    if (!r?.file.length) return
    this.file = r.file
    this.contexte = r.contexte
    this.brancher()
    medias.demarre('sillon')
    void this.demarrer(Math.min(Math.max(0, r.index), r.file.length - 1), r.position)
  }

  basculer() { if (this.lecture) this.pause(); else this.reprendre() }

  pause() {
    const e = this.slots?.[this.actif]
    if (!e) return
    e.el.pause()
    this.etat(false)
    this.sauver()
  }

  reprendre() {
    const e = this.slots?.[this.actif]
    if (!e || !this.piste) return
    this.brancher()
    medias.demarre('sillon')
    if (e.lecture && e.piste?.id === this.piste.id) void this.jouerElement(e)
    else void this.demarrer(this.index, this.position)
  }

  debloquer() {
    this.bloque = false
    this.reprendre()
  }

  suivant() {
    if (this.index + 1 >= this.file.length) return
    this.brancher()
    void this.demarrer(this.index + 1)
  }

  precedent() {
    if (this.position > 3 || this.index <= 0) this.deplacer(0)
    else { this.brancher(); void this.demarrer(this.index - 1) }
  }

  aller(index: number) {
    if (index < 0 || index >= this.file.length) return
    this.brancher()
    medias.demarre('sillon')
    void this.demarrer(index)
  }

  deplacer(t: number) {
    const e = this.slots?.[this.actif]
    const s = Math.max(0, Math.min(t, this.duree || t))
    this.position = s
    if (e?.lecture) e.el.currentTime = s + debut(e.lecture)
  }

  regler(volume: number) {
    this.volume = Math.max(0, Math.min(1, volume))
    for (const e of this.slots ?? []) this.appliquerGain(e)
    ecrire(this.env.stockage, CLE_VOLUME, this.volume)
  }

  arreter() {
    this.tour++
    for (const e of this.slots ?? []) { e.jeton++; e.el.pause(); e.el.removeAttribute('src'); e.el.load(); e.piste = null; e.lecture = null }
    this.sauver()
    this.file = []
    this.index = -1
    this.position = 0
    this.duree = 0
    this.attente = false
    this.bloque = false
    this.etat(false)
    if (this.env.session) this.env.session.metadata = null
  }

  async chargerPreferences() {
    try { this.qualite = (await sillonApi.preferences()).quality } catch { this.qualite = null }
  }

  async changerQualite(q: Qualite) {
    this.qualite = (await sillonApi.qualite(q)).quality
    const e = this.slots?.[1 - this.actif]
    if (e) { e.jeton++; e.piste = null; e.lecture = null }
    if (this.lecture) this.precharger()
  }

  sauver() {
    if (!this.piste) return
    const debutFile = Math.max(0, Math.min(this.index, this.file.length - MAX_REPRISE))
    this.reprise = { file: this.file.slice(debutFile, debutFile + MAX_REPRISE), index: this.index - debutFile, position: this.position, contexte: this.contexte }
    ecrire(this.env.stockage, CLE_REPRISE, this.reprise)
  }

  private etat(lecture: boolean) {
    this.lecture = lecture
    if (this.env.session) this.env.session.playbackState = this.piste ? (lecture ? 'playing' : 'paused') : 'none'
  }

  private estActif(e: Emplacement) { return this.slots?.[this.actif] === e }

  private emplacements(): [Emplacement, Emplacement] {
    if (this.slots) return this.slots
    const creer = (): Emplacement => {
      const el = this.env.creerAudio()
      el.crossOrigin = 'anonymous'
      el.preload = 'auto'
      const e: Emplacement = { el, piste: null, lecture: null, gain: null, jeton: 0 }
      el.addEventListener('timeupdate', () => {
        if (!this.estActif(e) || !e.lecture) return
        this.position = Math.max(0, el.currentTime - debut(e.lecture))
        this.positionSession()
      })
      el.addEventListener('playing', () => {
        if (!this.estActif(e)) return
        this.attente = false
        this.bloque = false
        this.etat(true)
        this.precharger()
      })
      el.addEventListener('waiting', () => { if (this.estActif(e)) this.attente = true })
      el.addEventListener('pause', () => { if (this.estActif(e) && !el.ended) this.etat(false) })
      el.addEventListener('ended', () => { if (this.estActif(e)) this.fin() })
      el.addEventListener('error', () => {
        if (!this.estActif(e) || !e.lecture) return
        this.erreur = 'Cette piste ne peut pas être lue.'
        this.attente = false
        this.etat(false)
      })
      return e
    }
    this.slots = [creer(), creer()]
    const el = this.slots[0].el
    this.formats ??= support((t) => el.canPlayType(t))
    return this.slots
  }

  private brancher() {
    const slots = this.emplacements()
    if (this.ctx === undefined) {
      try { this.ctx = this.env.contexteAudio?.() ?? null } catch { this.ctx = null }
    }
    const ctx = this.ctx
    if (!ctx) return
    const relier = () => {
      if (ctx.state !== 'running') return
      for (const e of slots) {
        if (e.gain) continue
        try {
          const g = ctx.createGain()
          ctx.createMediaElementSource(e.el as never).connect(g)
          g.connect(ctx.destination)
          e.gain = g
        } catch { e.gain = null }
        this.appliquerGain(e)
      }
    }
    if (ctx.state === 'running') relier()
    else void ctx.resume().then(relier, () => {})
  }

  private appliquerGain(e: Emplacement) {
    const lin = e.lecture ? gainLineaire(e.lecture, this.contexte.type === 'album') : 1
    if (e.gain) { e.gain.gain.value = lin * this.volume; e.el.volume = 1 }
    else e.el.volume = Math.min(1, lin) * this.volume
  }

  private async charger(e: Emplacement, piste: Piste): Promise<boolean> {
    const jeton = ++e.jeton
    e.piste = piste
    e.lecture = null
    const l = await this.env.ouvrir(piste.id, { ...(this.formats ?? { codecs: [], containers: [] }), ...(this.qualite ? { quality: this.qualite } : {}) })
    if (jeton !== e.jeton) return false
    e.lecture = l
    e.el.src = this.env.base + l.url
    e.el.load()
    const d = debut(l)
    if (d) e.el.currentTime = d
    this.appliquerGain(e)
    return true
  }

  private precharger() {
    const p = this.file[this.index + 1]
    const e = this.slots?.[1 - this.actif]
    if (!p || !e || e.piste?.id === p.id) return
    void this.charger(e, p).catch(() => { if (e.piste?.id === p.id) e.piste = null })
  }

  private async demarrer(index: number, position = 0) {
    const tour = ++this.tour
    const slots = this.emplacements()
    const p = this.file[index]
    if (!p) return
    this.index = index
    this.position = position
    this.duree = p.duration ?? 0
    this.erreur = ''
    const autre = slots[1 - this.actif]
    const pret = autre.piste?.id === p.id && !!autre.lecture && position === 0
    const ancien = slots[this.actif]
    const e = pret ? autre : ancien
    if (pret) { ancien.el.pause(); this.actif = 1 - this.actif }
    this.metadonnees()
    if (!pret) {
      this.attente = true
      try {
        if (!(await this.charger(e, p)) || tour !== this.tour) return
      } catch (err) {
        if (tour !== this.tour) return
        this.erreur = err instanceof ApiError && err.status === 503 ? 'Le serveur est occupé, réessaie dans quelques secondes.' : 'Cette piste ne peut pas être lue.'
        this.attente = false
        this.etat(false)
        return
      }
      if (position && e.lecture) e.el.currentTime = position + debut(e.lecture)
    }
    this.duree = dureeExacte(e.lecture, p)
    this.sauver()
    await this.jouerElement(e)
  }

  private async jouerElement(e: Emplacement) {
    if (this.ctx && this.ctx.state !== 'running') void this.ctx.resume().catch(() => {})
    try {
      await e.el.play()
      if (!this.estActif(e)) return
      this.bloque = false
      this.etat(true)
    } catch (err) {
      if (!this.estActif(e)) return
      this.attente = false
      this.etat(false)
      const nom = (err as { name?: string } | null)?.name
      if (nom === 'NotAllowedError') this.bloque = true
      else if (nom !== 'AbortError') this.erreur = 'Cette piste ne peut pas être lue.'
    }
  }

  private fin() {
    if (this.index + 1 < this.file.length) { void this.demarrer(this.index + 1); return }
    this.etat(false)
    this.position = 0
    this.deplacer(0)
  }

  private positionSession() {
    const s = this.env.session
    if (!s?.setPositionState || !(this.duree > 0)) return
    try { s.setPositionState({ duration: this.duree, position: Math.min(this.position, this.duree), playbackRate: 1 }) } catch { return }
  }

  private actions() {
    const s = this.env.session
    if (!s || this.actionsPosees) return
    this.actionsPosees = true
    const poser = (a: string, f: (d: DetailsAction) => void) => { try { s.setActionHandler(a, f) } catch { return } }
    poser('play', () => this.reprendre())
    poser('pause', () => this.pause())
    poser('previoustrack', () => this.precedent())
    poser('nexttrack', () => this.suivant())
    poser('seekto', (d) => { if (typeof d.seekTime === 'number') this.deplacer(d.seekTime) })
    poser('seekbackward', (d) => this.deplacer(this.position - (d.seekOffset || 10)))
    poser('seekforward', (d) => this.deplacer(this.position + (d.seekOffset || 10)))
  }

  private metadonnees() {
    const s = this.env.session
    const p = this.piste
    if (!s || !p) return
    this.actions()
    const poser = (src?: string) => {
      s.metadata = this.env.metadonnees?.({ title: p.title, artist: p.artist.name, album: p.album.title, artwork: src ? [{ src, sizes: '512x512' }] : [] }) ?? null
    }
    if (this.art?.album === p.album.id) { poser(this.art.url); return }
    poser()
    if (!p.album.cover || !this.env.pochette) return
    void this.env.pochette(p.album.id).then((url) => {
      if (this.art) URL.revokeObjectURL(this.art.url)
      this.art = { album: p.album.id, url }
      if (this.piste?.id === p.id) poser(url)
    }).catch(() => {})
  }
}

const navigateur = typeof window !== 'undefined'

export const lecteur = new Lecteur({
  creerAudio: () => new Audio(),
  contexteAudio: () => (navigateur && 'AudioContext' in window ? (new AudioContext() as unknown as ContexteAudio) : null),
  ouvrir: (id, demande) => sillonApi.lecture(id, demande),
  base: SILLON_URL,
  session: navigateur && 'mediaSession' in navigator ? (navigator.mediaSession as unknown as SessionMedia) : null,
  metadonnees: (m) => (typeof MediaMetadata === 'undefined' ? null : new MediaMetadata(m)),
  pochette: (album) => sillonApi.pochette(album, 'medium'),
  stockage: (() => { try { return navigateur ? localStorage : null } catch { return null } })(),
})

if (navigateur) window.addEventListener('pagehide', () => lecteur.sauver())

export const vue = $state({ refuse: false })

export function echec(e: unknown, message: string) {
  if (e instanceof ApiError && e.status === 403) vue.refuse = true
  else notify.error(e, message)
}
