import { ApiError } from '../../lib/api'
import { medias } from '../../lib/medias'
import { confirmer, saisir } from '../../lib/confirm.svelte'
import type { MenuItem } from '../../lib/ctxmenu.svelte'
import { hrefs, router } from '../../lib/route.svelte'
import { notify } from '../../lib/ui'
import { debut, dureeExacte, gainLineaire, SILLON_URL, sillonApi, support, type Contenu, type DemandeLecture, type FichePlaylist, type Playlist, titresDe, type EcouteEnvoyee, type ElementFile, type FileServeur, type Lecture, type Piste, type Qualite, REPETITIONS, type Repetition, type Suggestions, type Support, type TypeSource } from './sillon'

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
  serveur?: ServeurFile | null
  identifiant?(): string
}

type Corps = Record<string, unknown>

export interface ServeurFile {
  lire(): Promise<FileServeur>
  remplacer(b: Corps): Promise<FileServeur>
  modifier(b: Corps): Promise<FileServeur>
  ajouter(b: Corps): Promise<FileServeur>
  retirer(b: Corps): Promise<FileServeur>
  deplacer(b: Corps): Promise<FileServeur>
  ecoute(b: EcouteEnvoyee): Promise<unknown>
}

export type Contexte = { type: 'album' | 'playlist' | 'artist'; id: string } | { type: 'likes' } | { type: 'library' } | { type: 'pistes'; query?: string }

export function sourceDe(c: Contexte): { type: TypeSource; id?: string; query?: string } | undefined {
  if (c.type === 'pistes') return c.query ? { type: 'search', query: c.query } : undefined
  return c.type === 'likes' || c.type === 'library' ? { type: c.type } : { type: c.type, id: c.id }
}

export function contexteDe(s: FileServeur['source']): Contexte {
  if (!s) return { type: 'pistes' }
  if (s.type === 'search') return { type: 'pistes', query: s.name }
  if (s.type === 'likes' || s.type === 'library') return { type: s.type }
  return s.id ? { type: s.type, id: s.id } : { type: 'pistes' }
}

export function conflit(e: unknown): FileServeur | null {
  const q = e instanceof ApiError && e.status === 409 ? (e.data.queue as FileServeur | undefined) : undefined
  return q && Array.isArray(q.items) ? q : null
}

const ENVOI_POSITION = 15

interface Emplacement { el: ElementAudio; piste: Piste | null; lecture: Lecture | null; gain: NoeudGain | null; jeton: number }

const CLE_VOLUME = 'sillon.volume'

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
  decouverte = $state(true)
  erreur = $state('')
  elements = $state.raw<ElementFile[]>([])
  serveur = $state.raw<FileServeur | null>(null)
  aReprendre = $state(false)

  private slots: [Emplacement, Emplacement] | null = null
  private actif = 0
  private ctx: ContexteAudio | null | undefined
  private formats: Support | null = null
  private tour = 0
  private actionsPosees = false
  private art: { album: string; url: string } | null = null
  private ecritures: Promise<unknown> = Promise.resolve()
  private envoye = 0
  private ecoute: { piste: Piste; element: string | undefined; joue: number; dernier: number } | null = null

  constructor(private env: Environnement) {
    medias.ecouter((s) => { if (s !== 'sillon') this.pause() })
    const v = lireJson<number>(env.stockage, CLE_VOLUME)
    if (typeof v === 'number' && v >= 0 && v <= 1) this.volume = v
  }

  get piste(): Piste | null { return this.file[this.index] ?? null }
  get aSuivre(): Piste[] { return this.file.slice(this.index + 1) }
  get elementActif(): ElementAudio | null { return this.slots?.[this.actif].el ?? null }
  get elementSuivant(): ElementAudio | null { return this.slots?.[1 - this.actif].el ?? null }
  get element(): ElementFile | null { return this.elements[this.index] ?? null }
  get aleatoire() { return !!this.serveur?.shuffle }
  get suggestions(): Suggestions { return this.serveur?.suggestions ?? 'off' }
  get sansFin() { return this.serveur?.endless ?? true }
  get repetition(): Repetition { return this.serveur?.repeat ?? 'off' }
  get suivante(): Piste | null { return this.file[this.prochain(true)] ?? null }
  get peutSuivre() { return this.prochain(false) >= 0 }

  jouer(pistes: Piste[], depart = 0, contexte: Contexte = { type: 'pistes' }, melanger = false) {
    const dispo = pistes.filter((p) => p.available)
    if (!dispo.length) return
    this.cloturer('skip')
    const index = Math.max(0, dispo.indexOf(pistes[depart]))
    this.file = dispo
    this.elements = []
    this.contexte = contexte
    this.aReprendre = false
    this.brancher()
    medias.demarre('sillon')
    void this.demarrer(index)
    const source = sourceDe(contexte)
    void this.ecrireFile((s, revision) => s.remplacer({ revision, tracks: dispo.map((p) => p.id), index, ...(source ? { source } : {}), ...(melanger ? { shuffle: true } : {}) })).catch(() => {})
  }

  async melangerTout() {
    if (!this.env.serveur) return 0
    this.cloturer('skip')
    this.arreter()
    this.brancher()
    await this.ecrireFile((s, revision) => s.remplacer({ revision, source: { type: 'library' } }))
    if (!this.piste) return 0
    this.reprendre()
    return this.file.length
  }

  async chargerFile() {
    const s = this.env.serveur
    if (!s) return
    try {
      const f = await s.lire()
      if (!this.piste && !this.serveur) this.recevoir(f)
    } catch { return }
  }

  basculerAleatoire() { return this.ecrireFile((s, revision) => s.modifier({ revision, shuffle: !this.aleatoire })) }
  reglerSansFin(v: boolean) { return this.ecrireFile((s, revision) => s.modifier({ revision, endless: v })) }
  reglerSuggestions(v: Suggestions) { return this.ecrireFile((s, revision) => s.modifier({ revision, suggestions: v })) }
  changerRepetition() {
    const k = REPETITIONS.findIndex((r) => r.id === this.repetition)
    const repeat = REPETITIONS[(k + 1) % REPETITIONS.length].id
    return this.ecrireFile((s, revision) => s.modifier({ revision, repeat }))
  }
  lireEnsuite(c: Contenu) { return this.ecrireFile((s, revision) => s.ajouter({ revision, mode: 'next', ...c })) }
  ajouterAFile(c: Contenu) { return this.ecrireFile((s, revision) => s.ajouter({ revision, mode: 'end', ...c })) }
  retirer(id: string) { return this.ecrireFile((s, revision) => s.retirer({ revision, items: [id] })) }

  async nePlusProposer(e: ElementFile) {
    await sillonApi.nePlusProposer(e.track.id)
    await this.retirer(e.id)
  }

  async similaires(p: Piste) {
    const { items } = await sillonApi.similaires(p.id)
    this.jouer(items)
    return items.filter((x) => x.available).length
  }

  deplacerElement(id: string, versId: string) {
    return this.ecrireFile((s, revision, f) => {
      const from = f?.items.findIndex((e) => e.id === id) ?? -1
      const to = f?.items.findIndex((e) => e.id === versId) ?? -1
      return from < 0 || to < 0 || from === to ? null : s.deplacer({ revision, from, to })
    })
  }

  quitter() {
    this.cloturer('stop')
    this.envoyerPosition()
  }

  basculer() { if (this.lecture) this.pause(); else this.reprendre() }

  pause() {
    const e = this.slots?.[this.actif]
    if (!e) return
    e.el.pause()
    this.etat(false)
    this.envoyerPosition()
  }

  reprendre() {
    if (!this.piste) return
    const e = this.emplacements()[this.actif]
    this.brancher()
    medias.demarre('sillon')
    this.aReprendre = false
    if (e.lecture && e.piste?.id === this.piste.id) void this.jouerElement(e)
    else void this.demarrer(this.index, this.position)
  }

  debloquer() {
    this.bloque = false
    this.reprendre()
  }

  suivant() {
    const i = this.prochain(false)
    if (i < 0) return
    this.brancher()
    void this.demarrer(i)
  }

  precedent() {
    if (this.position > 3 || (this.index <= 0 && this.repetition === 'off') || this.file.length < 2) this.deplacer(0)
    else { this.brancher(); void this.demarrer(this.index > 0 ? this.index - 1 : this.file.length - 1) }
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
    if (this.ecoute) this.ecoute.dernier = s
    if (e?.lecture) e.el.currentTime = s + debut(e.lecture)
  }

  regler(volume: number) {
    this.volume = Math.max(0, Math.min(1, volume))
    for (const e of this.slots ?? []) this.appliquerGain(e)
    ecrire(this.env.stockage, CLE_VOLUME, this.volume)
  }

  arreter() {
    this.quitter()
    this.tour++
    for (const e of this.slots ?? []) { e.jeton++; e.el.pause(); e.el.removeAttribute('src'); e.el.load(); e.piste = null; e.lecture = null }
    this.file = []
    this.elements = []
    this.aReprendre = false
    this.index = -1
    this.position = 0
    this.duree = 0
    this.attente = false
    this.bloque = false
    this.etat(false)
    if (this.env.session) this.env.session.metadata = null
  }

  async chargerPreferences() {
    try {
      const p = await sillonApi.preferences()
      this.qualite = p.quality
      this.decouverte = p.discovery
    } catch { this.qualite = null }
  }

  async changerDecouverte(v: boolean) {
    this.decouverte = (await sillonApi.decouverteActive(v)).discovery
  }

  async changerQualite(q: Qualite) {
    this.qualite = (await sillonApi.qualite(q)).quality
    const e = this.slots?.[1 - this.actif]
    if (e) { e.jeton++; e.piste = null; e.lecture = null }
    if (this.lecture) this.precharger()
  }

  private ecrireFile(faire: (s: ServeurFile, revision: number, f: FileServeur | null) => Promise<FileServeur> | null): Promise<void> {
    const s = this.env.serveur
    if (!s) return Promise.resolve()
    const t = this.ecritures.then(async () => {
      for (let essai = 0; essai < 2; essai++) {
        const p = faire(s, this.serveur?.revision ?? 0, this.serveur)
        if (!p) return
        try {
          this.recevoir(await p)
          return
        } catch (e) {
          if (essai || !(e instanceof ApiError && e.status === 409)) throw e
          this.recevoir(conflit(e) ?? (await s.lire()))
        }
      }
    })
    this.ecritures = t.catch(() => {})
    return t
  }

  private indexServeur(f: FileServeur, dispo: ElementFile[]) {
    for (let k = Math.max(0, f.index); k < f.items.length; k++) {
      const i = dispo.indexOf(f.items[k])
      if (i >= 0) return i
    }
    return dispo.length ? 0 : -1
  }

  private recevoir(f: FileServeur) {
    this.serveur = f
    const dispo = f.items.filter((e) => e.track.available)
    const cur = this.element
    const p = this.piste
    const srv = this.indexServeur(f, dispo)
    let i = cur ? dispo.findIndex((e) => e.id === cur.id) : -1
    if (i < 0 && p && !cur) i = dispo[srv]?.track.id === p.id ? srv : dispo.findIndex((e) => e.track.id === p.id)
    this.elements = dispo
    this.file = dispo.map((e) => e.track)
    if (i >= 0) {
      this.index = i
      const ec = this.ecoute
      if (ec && !ec.element && dispo[i].track.id === ec.piste.id) ec.element = dispo[i].id
      if (this.lecture) this.precharger()
      this.signalerPiste()
      return
    }
    this.index = srv
    if (p) {
      if (this.lecture && srv >= 0) void this.demarrer(srv)
      return
    }
    this.contexte = contexteDe(f.source)
    this.position = srv >= 0 && dispo[srv] === f.items[f.index] ? f.position : 0
    this.duree = dispo[srv]?.track.duration ?? 0
    this.aReprendre = srv >= 0
  }

  private positionServeur(f: FileServeur | null) {
    const id = this.element?.id
    const k = id ? f?.items.findIndex((e) => e.id === id) ?? -1 : -1
    return k
  }

  private signalerPiste() {
    const f = this.serveur
    const k = this.positionServeur(f)
    if (!f || k < 0 || k === f.index) return
    this.envoyerPosition()
  }

  private envoyerPosition() {
    if (!this.serveur || !this.element) return
    this.envoye = this.position
    const position = Math.round(this.position * 10) / 10
    void this.ecrireFile((s, revision, f) => {
      const index = this.positionServeur(f)
      if (index < 0 || !f) return null
      return s.modifier(index === f.index ? { revision, position } : { revision, index, ...(position ? { position } : {}) })
    }).catch(() => {})
  }

  private prochain(fin: boolean) {
    const n = this.file.length
    if (!n || this.index < 0) return -1
    if (fin && this.repetition === 'one') return this.index
    if (this.index + 1 < n) return this.index + 1
    return this.repetition === 'off' ? -1 : 0
  }

  private cloturer(fin: EcouteEnvoyee['ended_by']) {
    const e = this.ecoute
    this.ecoute = null
    const s = this.env.serveur
    if (!e || !s || e.joue < 1) return
    const source = sourceDe(this.contexte)
    const id = this.env.identifiant?.() ?? crypto.randomUUID()
    void s.ecoute({ id, track: e.piste.id, played: Math.round(e.joue * 10) / 10, ended_by: fin, ...(source ? { source: { type: source.type, ...(source.id ? { id: source.id } : {}) } } : {}), ...(e.element ? { item: e.element } : {}) }).catch(() => {})
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
        const ec = this.ecoute
        if (ec && ec.piste.id === e.piste?.id) {
          const d = this.position - ec.dernier
          if (d > 0 && d < 3) ec.joue += d
          ec.dernier = this.position
        }
        if (this.lecture && Math.abs(this.position - this.envoye) >= ENVOI_POSITION) this.envoyerPosition()
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
    const p = this.file[this.prochain(true)]
    const e = this.slots?.[1 - this.actif]
    if (!p || !e || e.piste?.id === p.id) return
    void this.charger(e, p).catch(() => { if (e.piste?.id === p.id) e.piste = null })
  }

  private async demarrer(index: number, position = 0) {
    const tour = ++this.tour
    const slots = this.emplacements()
    const p = this.file[index]
    if (!p) return
    this.cloturer('skip')
    this.index = index
    this.position = position
    this.envoye = position
    this.ecoute = { piste: p, element: this.elements[index]?.id, joue: 0, dernier: position }
    this.duree = p.duration ?? 0
    this.erreur = ''
    this.signalerPiste()
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
    this.cloturer('end')
    const i = this.prochain(true)
    if (i >= 0) { void this.demarrer(i); return }
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
  serveur: {
    lire: () => sillonApi.file(),
    remplacer: (b) => sillonApi.remplacerFile(b),
    modifier: (b) => sillonApi.modifierFile(b),
    ajouter: (b) => sillonApi.ajouterAFile(b),
    retirer: (b) => sillonApi.retirerDeFile(b),
    deplacer: (b) => sillonApi.deplacerDansFile(b),
    ecoute: (b) => sillonApi.enregistrerEcoute(b),
  },
})

if (navigateur) window.addEventListener('pagehide', () => lecteur.quitter())

export const vue = $state({ refuse: false })

export function echec(e: unknown, message: string) {
  if (e instanceof ApiError && e.status === 403) vue.refuse = true
  else notify.error(e, message)
}

export type GenreLike = 'tracks' | 'albums' | 'artists'

class Aimes {
  tracks = $state.raw(new Set<string>())
  albums = $state.raw(new Set<string>())
  artists = $state.raw(new Set<string>())
  private charge: Promise<void> | null = null

  charger(forcer = false) {
    if (this.charge && !forcer) return this.charge
    this.charge = (async () => {
      try {
        const l = await sillonApi.likes()
        this.tracks = new Set(l.tracks)
        this.albums = new Set(l.albums)
        this.artists = new Set(l.artists)
      } catch { this.charge = null }
    })()
    return this.charge
  }

  a(kind: GenreLike, id: string) { return this[kind].has(id) }

  async basculer(kind: GenreLike, id: string) {
    const oui = !this[kind].has(id)
    const poser = (v: boolean) => { const s = new Set(this[kind]); if (v) s.add(id); else s.delete(id); this[kind] = s }
    poser(oui)
    try { await sillonApi.liker(kind, id, oui) } catch (e) { poser(!oui); notify.error(e, 'Le like n’a pas pu être enregistré') }
  }
}

export const aimes = new Aimes()

class Collection {
  playlists = $state.raw<Playlist[]>([])
  charge = $state(false)

  async charger() {
    try { this.playlists = (await sillonApi.playlists()).items; this.charge = true } catch (e) { echec(e, 'Les playlists sont indisponibles') }
  }

  maj(p: Playlist) {
    const { items: _, ...r } = p as FichePlaylist
    this.playlists = [r, ...this.playlists.filter((x) => x.id !== p.id)]
  }

  async creer(): Promise<FichePlaylist | null> {
    const nom = await saisir({ titre: 'Nouvelle playlist', action: 'Créer', saisie: { label: 'Nom de la playlist' } })
    if (!nom?.trim()) return null
    try {
      const p = await sillonApi.creerPlaylist(nom.trim())
      this.maj(p)
      return p
    } catch (e) { notify.error(e, 'La playlist n’a pas pu être créée'); return null }
  }

  async renommer(p: Playlist): Promise<FichePlaylist | null> {
    const nom = await saisir({ titre: 'Renommer la playlist', action: 'Renommer', saisie: { label: 'Nom de la playlist', valeur: p.name } })
    if (!nom?.trim() || nom.trim() === p.name) return null
    try {
      const r = await sillonApi.renommerPlaylist(p.id, nom.trim())
      this.maj(r)
      return r
    } catch (e) { notify.error(e, 'La playlist n’a pas pu être renommée'); return null }
  }

  async supprimer(p: Playlist) {
    if (!(await confirmer({ titre: `Supprimer « ${p.name} » ?`, message: 'La playlist et son ordre seront perdus. Les pistes restent dans la bibliothèque.', action: 'Supprimer', destructif: true }))) return false
    try {
      await sillonApi.supprimerPlaylist(p.id)
      this.playlists = this.playlists.filter((x) => x.id !== p.id)
      return true
    } catch (e) { notify.error(e, 'La playlist n’a pas pu être supprimée'); return false }
  }

  async ajouter(id: string, contenu: Contenu) {
    try {
      let r = await sillonApi.ajouterAPlaylist(id, contenu)
      if (!r.added && r.duplicates.length) {
        if (!(await confirmer({ titre: 'Déjà dans la playlist', message: r.duplicates.length > 1 ? `${r.duplicates.length} titres y sont déjà. Les ajouter quand même ?` : 'Ce titre y est déjà. L’ajouter quand même ?', action: 'Ajouter quand même' }))) return null
        r = await sillonApi.ajouterAPlaylist(id, contenu, true)
      }
      this.maj(r)
      notify.ok(`${titresDe(r.added)} ajouté${r.added > 1 ? 's' : ''} à « ${r.name} ».`)
      return r
    } catch (e) { notify.error(e, 'L’ajout à la playlist a échoué'); return null }
  }
}

export const collection = new Collection()

export const choix = $state<{ contenu: Contenu | null; titre: string }>({ contenu: null, titre: '' })

export const lien = (section: string, id: string) => `${location.origin}${location.pathname}${hrefs.musique(section, id)}`

export function copierLien(section: string, id: string) {
  void navigator.clipboard.writeText(lien(section, id)).then(() => notify.ok('Lien copié.'), (e) => notify.error(e, 'Copie impossible'))
}

const action = (p: Promise<unknown>, message: string, erreur: string) => void p.then(() => notify.ok(message), (e) => notify.error(e, erreur))

export function menuContenu(contenu: Contenu, titre: string): MenuItem[] {
  return [
    { label: 'Lire ensuite', run: () => action(lecteur.lireEnsuite(contenu), 'Ajouté après le titre en cours.', 'La file n’a pas pu être modifiée') },
    { label: 'Ajouter à la file', run: () => action(lecteur.ajouterAFile(contenu), 'Ajouté à la file.', 'La file n’a pas pu être modifiée') },
    { label: 'Ajouter à une playlist…', run: () => { choix.contenu = contenu; choix.titre = titre } },
  ]
}

export function menuPiste(p: Piste, extra: MenuItem[] = []): MenuItem[] {
  return [
    ...(p.available ? menuContenu({ tracks: [p.id] }, p.title) : []),
    ...(p.available ? [{ label: 'Titres similaires', run: () => void lecteur.similaires(p).then((n) => { if (!n) notify.info('Aucun titre similaire pour l’instant : l’analyse de ce titre n’est peut-être pas terminée.') }, (e) => notify.error(e, 'Les titres similaires sont indisponibles')) }] : []),
    ...extra,
    { label: aimes.a('tracks', p.id) ? 'Retirer des titres likés' : 'Ajouter aux titres likés', run: () => void aimes.basculer('tracks', p.id), sep: true },
    { label: 'Voir l’album', run: () => router.go(hrefs.musique('album', p.album.id)) },
    { label: 'Voir l’artiste', run: () => router.go(hrefs.musique('artist', p.artist.id)) },
    { label: 'Copier le lien', run: () => copierLien('track', p.id) },
  ]
}
