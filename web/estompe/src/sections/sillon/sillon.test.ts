import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { medias } from '../../lib/medias'
import { contexteDe, Lecteur, sourceDe, type ContexteAudio, type DetailsAction, type ElementAudio, type Environnement, type NoeudGain, type ServeurFile } from './sillon.svelte'
import { anneeEtType, debut, deplacerListe, dureeExacte, gainLineaire, marqueDe, octets, refusAjout, sections, sillonApi, support, totalAlbum, type EcouteEnvoyee, type ElementFile, type FileServeur, type Lecture, type Piste } from './sillon'

class FauxAudio implements ElementAudio {
  src = ''
  crossOrigin: string | null = null
  preload = ''
  currentTime = 0
  volume = 1
  paused = true
  ended = false
  lectures = 0
  refus: string | null = null
  private ecouteurs = new Map<string, (() => void)[]>()
  play() {
    this.lectures++
    if (this.refus) return Promise.reject(Object.assign(new Error('refus'), { name: this.refus }))
    this.paused = false
    return Promise.resolve()
  }
  pause() { this.paused = true }
  load() {}
  removeAttribute(n: string) { if (n === 'src') this.src = '' }
  canPlayType(t: string) { return t === 'audio/mpeg' || t.startsWith('audio/ogg; codecs="opus"') ? 'probably' : '' }
  addEventListener(t: string, f: () => void) { this.ecouteurs.set(t, [...(this.ecouteurs.get(t) ?? []), f]) }
  emettre(t: string) { for (const f of this.ecouteurs.get(t) ?? []) f() }
}

const piste = (n: number, o: Partial<Piste> = {}): Piste => ({
  id: `p${n}`, title: `Titre ${n}`, disc: 1, number: n, duration: 180, explicit: false, available: true,
  album: { id: 'a1', title: 'Album', year: 2020, cover: true }, artist: { id: 'r1', name: 'Artiste' }, format: null, ...o,
})

const lecture = (id: string, o: Partial<Lecture> = {}): Lecture => ({
  track: id, url: `/stream/${id}`, expires_at: '', mode: 'direct', quality: 'originale', format: { codec: 'mp3', container: 'mp3', mime: 'audio/mpeg', bitrate: 320000 },
  duration: 180, samples: 180 * 44100, sample_rate: 44100, output_sample_rate: 44100, encoder_delay: 0,
  gain: { reference: -18, loudness: -10, track: -8, track_peak: 0.5, album: -6, album_peak: 1 }, ...o,
})

const attendre = async (ok: () => boolean) => { await vi.waitFor(() => { if (!ok()) throw new Error('pas encore') }) }

function banc(o: Partial<Environnement> = {}, l: (id: string) => Partial<Lecture> = () => ({})) {
  const audios: FauxAudio[] = []
  const handlers = new Map<string, (d: DetailsAction) => void>()
  const session = { metadata: null as unknown, playbackState: 'none', setActionHandler: (a: string, f: ((d: DetailsAction) => void) | null) => { if (f) handlers.set(a, f) }, setPositionState: vi.fn() }
  const ouvrir = vi.fn(async (id: string) => lecture(id, l(id)))
  const stock = new Map<string, string>()
  const env: Environnement = {
    creerAudio: () => { const a = new FauxAudio(); audios.push(a); return a },
    ouvrir,
    base: 'https://sillon.test',
    session,
    metadonnees: (m) => m,
    pochette: async (album) => `blob:${album}`,
    stockage: { getItem: (k) => stock.get(k) ?? null, setItem: (k, v) => { stock.set(k, v) } },
    ...o,
  }
  return { lecteur: new Lecteur(env), audios, handlers, session, ouvrir, stock }
}

const album = [piste(1), piste(2), piste(3)]

afterEach(() => vi.restoreAllMocks())

describe('client de l’API', () => {
  it('formats annoncés selon canPlayType', () => {
    const s = support((t) => (['audio/flac', 'audio/ogg; codecs="opus"', 'audio/mp4; codecs="mp4a.40.2"'].includes(t) ? 'maybe' : ''))
    expect(s.codecs.sort()).toEqual(['aac', 'flac', 'opus'])
    expect(s.containers.sort()).toEqual(['flac', 'mp4', 'ogg'])
    expect(support(() => '')).toEqual({ codecs: [], containers: [] })
  })

  it('demande de lecture : POST avec les formats et la qualité, réponse JSON', async () => {
    const f = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify(lecture('p1')), { headers: { 'Content-Type': 'application/json' } }))
    const r = await sillonApi.lecture('p1', { codecs: ['opus'], containers: ['ogg'], quality: 'basse' })
    expect(r.url).toBe('/stream/p1')
    const [url, init] = f.mock.calls[0]
    expect(String(url)).toMatch(/\/tracks\/p1\/playback$/)
    expect(init?.method).toBe('POST')
    expect(init?.credentials).toBe('include')
    expect(JSON.parse(String(init?.body))).toEqual({ codecs: ['opus'], containers: ['ogg'], quality: 'basse' })
  })

  it('pages par curseur et recherche encodée', async () => {
    const f = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response('{"items":[],"next_cursor":null}', { headers: { 'Content-Type': 'application/json' } }))
    await sillonApi.recents('abc', 40)
    await sillonApi.recherche('beyoncé & co')
    expect(String(f.mock.calls[0][0])).toMatch(/\/albums\/recent\?limit=40&cursor=abc$/)
    expect(String(f.mock.calls[1][0])).toContain('/search?q=beyonc%C3%A9%20%26%20co')
  })

  it('gain de normalisation : piste ou album, plafonné par le pic', () => {
    expect(gainLineaire(lecture('x'), false)).toBeCloseTo(10 ** (-8 / 20))
    expect(gainLineaire(lecture('x'), true)).toBeCloseTo(10 ** (-6 / 20))
    expect(gainLineaire(lecture('x', { gain: { reference: -18, loudness: -25, track: 7, track_peak: -3, album: null, album_peak: null } }), true)).toBeCloseTo(10 ** (2 / 20))
    expect(gainLineaire(lecture('x', { gain: { reference: -18, loudness: null, track: null, track_peak: null, album: null, album_peak: null } }), false)).toBe(1)
  })

  it('début et durée exacts (délai de l’encodeur, échantillons)', () => {
    expect(debut(lecture('x', { encoder_delay: 1024, output_sample_rate: 48000 }))).toBeCloseTo(1024 / 48000)
    expect(debut(lecture('x'))).toBe(0)
    expect(dureeExacte(lecture('x', { samples: 441000, sample_rate: 44100 }), null)).toBe(10)
    expect(dureeExacte(null, piste(1))).toBe(180)
  })

  it('sections et libellés', () => {
    const me = (access: boolean) => ({ sub: 's', name: 'n', email: '', permissions: { access, add: false, admin: false }, sections: [] })
    expect(sections(false, me(true))).toEqual({ messagerie: false, sillon: true })
    expect(sections(true, me(false))).toEqual({ messagerie: true, sillon: false })
    expect(sections(true, null)).toEqual({ messagerie: true, sillon: false })
    expect(anneeEtType({ year: 2016, type: 'Album', secondary_types: [] })).toBe('2016 · Album')
    expect(anneeEtType({ year: null, type: 'Album', secondary_types: ['Live'] })).toBe('Live')
    expect(totalAlbum([piste(1, { duration: 3000 }), piste(2, { duration: 1200 }), piste(3, { available: false })])).toBe('1 h 10')
  })
})

describe('lecteur', () => {
  it('un album se joue dans l’ordre ; la suivante est préchargée dès le début de la piste en cours', async () => {
    const b = banc()
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.audios[0]?.lectures === 1)
    const [a, c] = b.audios
    expect(a.src).toBe('https://sillon.test/stream/p1')
    expect(a.crossOrigin).toBe('anonymous')
    expect(b.ouvrir.mock.calls[0]).toEqual(['p1', { codecs: ['mp3', 'opus'], containers: ['mp3', 'ogg'] }])
    expect(c.src).toBe('')
    a.emettre('playing')
    await attendre(() => c.src === 'https://sillon.test/stream/p2')
    expect(b.lecteur.lecture).toBe(true)
    expect(c.lectures).toBe(0)

    a.paused = true
    a.ended = true
    a.emettre('pause')
    a.emettre('ended')
    expect(c.lectures).toBe(1)
    expect(b.lecteur.index).toBe(1)
    expect(b.lecteur.piste?.id).toBe('p2')
    expect(b.lecteur.elementActif).toBe(c)
    c.emettre('playing')
    await attendre(() => a.src === 'https://sillon.test/stream/p3')
    expect(b.ouvrir.mock.calls.map((x) => x[0])).toEqual(['p1', 'p2', 'p3'])
  })

  it('les pistes absentes sont sautées et la fin de la file arrête la lecture', async () => {
    const b = banc()
    b.lecteur.jouer([piste(1), piste(2, { available: false }), piste(3)], 2)
    await attendre(() => b.audios[0]?.lectures === 1)
    expect(b.lecteur.file.map((p) => p.id)).toEqual(['p1', 'p3'])
    expect(b.lecteur.piste?.id).toBe('p3')
    b.audios[0].emettre('playing')
    b.audios[0].emettre('ended')
    expect(b.lecteur.lecture).toBe(false)
    expect(b.lecteur.index).toBe(1)
  })

  it('suivant sans préchargement, précédent au début ou retour au début de la piste', async () => {
    const b = banc()
    b.lecteur.jouer(album)
    await attendre(() => b.audios[0]?.lectures === 1)
    b.lecteur.suivant()
    await attendre(() => b.lecteur.index === 1 && b.audios[0].src.endsWith('/p2'))
    b.lecteur.position = 10
    b.lecteur.precedent()
    expect(b.lecteur.index).toBe(1)
    expect(b.lecteur.position).toBe(0)
    b.lecteur.precedent()
    await attendre(() => b.lecteur.index === 0)
  })

  it('lecture automatique refusée : « Activer le son » au lieu d’une erreur, puis reprise par un geste', async () => {
    const b = banc()
    const premier = b.lecteur
    b.lecteur.jouer(album)
    await attendre(() => b.audios.length === 2)
    b.audios[0].refus = 'NotAllowedError'
    await attendre(() => premier.bloque)
    expect(premier.erreur).toBe('')
    expect(premier.lecture).toBe(false)
    b.audios[0].refus = null
    premier.debloquer()
    await attendre(() => premier.lecture && !premier.bloque)
    expect(b.audios[0].lectures).toBe(2)
  })

  it('délai de l’encodeur retiré au début du flux et dans la position', async () => {
    const b = banc({}, () => ({ encoder_delay: 1024, output_sample_rate: 48000, mode: 'transcode' }))
    b.lecteur.jouer(album)
    await attendre(() => b.audios[0]?.lectures === 1)
    expect(b.audios[0].currentTime).toBeCloseTo(1024 / 48000)
    b.audios[0].currentTime = 10 + 1024 / 48000
    b.audios[0].emettre('timeupdate')
    expect(b.lecteur.position).toBeCloseTo(10)
    b.lecteur.deplacer(30)
    expect(b.audios[0].currentTime).toBeCloseTo(30 + 1024 / 48000)
  })

  it('qualité choisie annoncée au serveur, et préchargement refait après un changement', async () => {
    const b = banc()
    vi.spyOn(sillonApi, 'qualite').mockResolvedValue({ quality: 'basse', qualities: [], discovery: true })
    b.lecteur.jouer(album)
    await attendre(() => b.audios[0]?.lectures === 1)
    b.audios[0].emettre('playing')
    await attendre(() => b.audios[1].src.endsWith('/p2'))
    await b.lecteur.changerQualite('basse')
    await attendre(() => b.ouvrir.mock.calls.length === 3)
    expect(b.ouvrir.mock.calls[2]).toEqual(['p2', expect.objectContaining({ quality: 'basse' })])
  })

  it('gain par Web Audio quand le contexte tourne', async () => {
    const gains: NoeudGain[] = []
    const ctx: ContexteAudio = {
      state: 'running', destination: {}, resume: async () => {},
      createMediaElementSource: () => ({ connect: () => ({}) }),
      createGain: () => { const g = { gain: { value: 1 }, connect: () => ({}) }; gains.push(g); return g },
    }
    const b = banc({ contexteAudio: () => ctx })
    b.lecteur.regler(0.5)
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.audios[0]?.lectures === 1)
    expect(gains).toHaveLength(2)
    expect(gains[0].gain.value).toBeCloseTo(10 ** (-6 / 20) * 0.5)
    expect(b.audios[0].volume).toBe(1)
  })

  it('contexte audio suspendu faute de geste : lecture quand même, gain par le volume de l’élément', async () => {
    const source = vi.fn()
    const ctx: ContexteAudio = { state: 'suspended', destination: {}, resume: async () => {}, createMediaElementSource: source, createGain: vi.fn() }
    const b = banc({ contexteAudio: () => ctx })
    b.lecteur.jouer(album)
    await attendre(() => b.audios[0]?.lectures === 1)
    expect(source).not.toHaveBeenCalled()
    expect(b.audios[0].volume).toBeCloseTo(10 ** (-8 / 20))
  })

  it('Media Session : métadonnées et actions pilotent le lecteur', async () => {
    const b = banc()
    b.lecteur.jouer(album)
    await attendre(() => b.audios[0]?.lectures === 1)
    await attendre(() => (b.session.metadata as { artwork: unknown[] } | null)?.artwork.length === 1)
    expect(b.session.metadata).toEqual({ title: 'Titre 1', artist: 'Artiste', album: 'Album', artwork: [{ src: 'blob:a1', sizes: '512x512' }] })
    expect([...b.handlers.keys()].sort()).toEqual(['nexttrack', 'pause', 'play', 'previoustrack', 'seekbackward', 'seekforward', 'seekto'])
    expect(b.session.playbackState).toBe('playing')

    b.handlers.get('pause')!({})
    expect(b.audios[0].paused).toBe(true)
    expect(b.session.playbackState).toBe('paused')
    b.handlers.get('play')!({})
    await attendre(() => !b.audios[0].paused)
    b.handlers.get('seekto')!({ seekTime: 42 })
    expect(b.audios[0].currentTime).toBe(42)
    b.handlers.get('nexttrack')!({})
    await attendre(() => b.lecteur.piste?.id === 'p2')
    expect((b.session.metadata as { title: string }).title).toBe('Titre 2')
    b.handlers.get('previoustrack')!({})
    await attendre(() => b.lecteur.piste?.id === 'p1')
  })

  it('un film ou un salon vocal qui démarre met la musique en pause', async () => {
    const b = banc()
    b.lecteur.jouer(album)
    await attendre(() => b.lecteur.lecture)
    medias.demarre('film')
    expect(b.audios[0].paused).toBe(true)
    expect(b.lecteur.lecture).toBe(false)
    b.lecteur.reprendre()
    await attendre(() => b.lecteur.lecture)
    medias.demarre('vocal')
    expect(b.lecteur.lecture).toBe(false)
    medias.demarre('sillon')
    expect(b.lecteur.lecture).toBe(false)
  })

})

type Corps = Record<string, unknown>

function fauxServeur(catalogue: Piste[]) {
  let n = 0
  const el = (t: Piste, suggested = false, discovery = false): ElementFile => ({ id: `e${++n}`, suggested, discovery, track: t })
  const etat = { planifier: [] as ElementFile[], f: { revision: 0, source: null, index: 0, position: 0, shuffle: false, repeat: 'off', suggestions: 'off', seed: null, updated_at: null, items: [] } as FileServeur }
  const appels: [string, Corps][] = []
  const ecoutes: EcouteEnvoyee[] = []
  const piste = (id: unknown) => catalogue.find((p) => p.id === id)!
  const ecrire = (verbe: string, b: Corps, g: (f: FileServeur) => Partial<FileServeur>) => {
    appels.push([verbe, b])
    if (b.revision !== etat.f.revision) return Promise.reject(new ApiError(409, 'conflict', 'conflit', { queue: structuredClone(etat.f) }))
    etat.f = { ...etat.f, ...g(etat.f), revision: etat.f.revision + 1 }
    return Promise.resolve(structuredClone(etat.f))
  }
  const serveur: ServeurFile = {
    lire: async () => structuredClone(etat.f),
    remplacer: (b) => ecrire('PUT', b, () => ({ items: (b.tracks as string[]).map((id) => el(piste(id))), index: b.index as number, position: 0, source: b.source ? { type: 'album', id: (b.source as { id: string }).id, name: 'Album' } : null })),
    modifier: (b) => ecrire('PATCH', b, (f) => ({
      ...(b.index !== undefined ? { index: b.index as number, position: (b.position as number | undefined) ?? 0 } : b.position !== undefined ? { position: b.position as number } : {}),
      ...(b.index !== undefined && etat.planifier.length ? { items: [...f.items.slice(0, (b.index as number) + 1), ...etat.planifier.splice(0), ...f.items.slice((b.index as number) + 1)] } : {}),
      ...(b.repeat ? { repeat: b.repeat as FileServeur['repeat'] } : {}),
      ...(b.shuffle !== undefined ? { shuffle: b.shuffle as boolean, items: [f.items[f.index], ...f.items.filter((_, k) => k !== f.index).reverse()], index: 0 } : {}),
      ...(b.suggestions ? { suggestions: b.suggestions as FileServeur['suggestions'] } : {}),
    })),
    ajouter: (b) => ecrire('POST /queue/tracks', b, (f) => {
      const nouveaux = (b.tracks as string[]).map((id) => el(piste(id)))
      const k = b.mode === 'next' ? f.index + 1 : f.items.length
      return { items: [...f.items.slice(0, k), ...nouveaux, ...f.items.slice(k)] }
    }),
    retirer: (b) => ecrire('POST /queue/remove', b, (f) => ({ items: f.items.filter((e) => !(b.items as string[]).includes(e.id)) })),
    deplacer: (b) => ecrire('POST /queue/move', b, (f) => ({ items: deplacerListe(f.items, b.from as number, b.to as number) })),
    ecoute: async (b) => { ecoutes.push(b) },
  }
  return { serveur, appels, ecoutes, etat, el }
}

const catalogue = [piste(1), piste(2), piste(3), piste(4)]

describe('file serveur', () => {
  it('jouer remplace la file du serveur ; changement de piste et position envoyés', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur })
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.lecteur.elements.length === 3)
    expect(srv.appels[0]).toEqual(['PUT', { revision: 0, tracks: ['p1', 'p2', 'p3'], index: 0, source: { type: 'album', id: 'a1' } }])
    expect(b.lecteur.element?.id).toBe('e1')
    b.lecteur.suivant()
    await attendre(() => srv.etat.f.index === 1)
    expect(srv.appels[1]).toEqual(['PATCH', { revision: 1, index: 1 }])
    await attendre(() => b.audios[0]?.src.endsWith('/p2'))
    const a = b.audios[0]
    a.emettre('playing')
    a.currentTime = 16
    a.emettre('timeupdate')
    await attendre(() => srv.etat.f.position === 16)
    expect(srv.appels.at(-1)).toEqual(['PATCH', { revision: 2, position: 16 }])
    b.lecteur.pause()
    await attendre(() => srv.appels.length === 4)
  })

  it('409 : l’état courant est rechargé puis l’écriture rejouée', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur })
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.lecteur.serveur?.revision === 1)
    srv.etat.f = { ...srv.etat.f, revision: 7, items: [...srv.etat.f.items, srv.el(piste(4))] }
    await b.lecteur.basculerAleatoire()
    expect(srv.appels.slice(1).map(([v, c]) => [v, c.revision])).toEqual([['PATCH', 1], ['PATCH', 7]])
    expect(b.lecteur.aleatoire).toBe(true)
    expect(b.lecteur.file.map((p) => p.id)).toEqual(['p1', 'p4', 'p3', 'p2'])
    expect(b.lecteur.index).toBe(0)
    expect(b.audios[0].lectures).toBe(1)
  })

  it('reprise au démarrage : même piste et même position, proposée sans lecture automatique', async () => {
    const srv = fauxServeur(catalogue)
    srv.etat.f = { ...srv.etat.f, revision: 3, index: 1, position: 63, source: { type: 'album', id: 'a1', name: 'Album' }, items: [srv.el(piste(1)), srv.el(piste(2)), srv.el(piste(3))] }
    const b = banc({ serveur: srv.serveur })
    await b.lecteur.chargerFile()
    expect(b.lecteur.piste?.id).toBe('p2')
    expect(b.lecteur.position).toBe(63)
    expect(b.lecteur.aReprendre).toBe(true)
    expect(b.lecteur.contexte).toEqual({ type: 'album', id: 'a1' })
    expect(b.audios).toHaveLength(0)
    b.lecteur.reprendre()
    await attendre(() => b.audios[0]?.lectures === 1)
    expect(b.audios[0].src).toBe('https://sillon.test/stream/p2')
    expect(b.audios[0].currentTime).toBe(63)
    expect(b.lecteur.aReprendre).toBe(false)
    expect(srv.appels).toEqual([])
  })

  it('les pistes absentes de la file serveur sont passées', async () => {
    const srv = fauxServeur(catalogue)
    srv.etat.f = { ...srv.etat.f, revision: 1, index: 0, items: [srv.el(piste(1, { available: false })), srv.el(piste(2))] }
    const b = banc({ serveur: srv.serveur })
    await b.lecteur.chargerFile()
    expect(b.lecteur.file.map((p) => p.id)).toEqual(['p2'])
    expect(b.lecteur.position).toBe(0)
  })

  it('écoutes enregistrées : sautée, puis terminée, avec la source et l’élément', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur, identifiant: () => 'id' })
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.lecteur.elements.length === 3 && b.audios[0]?.lectures === 1)
    const a = b.audios[0]
    for (const t of [1, 2, 3, 4, 5]) { a.currentTime = t; a.emettre('timeupdate') }
    b.lecteur.suivant()
    expect(srv.ecoutes).toEqual([{ id: 'id', track: 'p1', played: 5, ended_by: 'skip', source: { type: 'album', id: 'a1' }, item: 'e1' }])
    await attendre(() => a.src.endsWith('/p2') && a.lectures === 2)
    a.emettre('playing')
    for (const t of [1, 2, 120, 121]) { a.currentTime = t; a.emettre('timeupdate') }
    a.emettre('ended')
    expect(srv.ecoutes[1]).toMatchObject({ track: 'p2', played: 3, ended_by: 'end', item: 'e2' })
  })

  it('lire ensuite, retirer et déplacer passent par le serveur ; suggestions marquées', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur })
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.lecteur.elements.length === 3)
    await b.lecteur.lireEnsuite({ tracks: ['p4'] })
    expect(srv.appels.at(-1)).toEqual(['POST /queue/tracks', { revision: 1, mode: 'next', tracks: ['p4'] }])
    expect(b.lecteur.aSuivre.map((p) => p.id)).toEqual(['p4', 'p2', 'p3'])
    await b.lecteur.deplacerElement('e4', 'e3')
    expect(srv.appels.at(-1)).toEqual(['POST /queue/move', { revision: 2, from: 1, to: 3 }])
    expect(b.lecteur.aSuivre.map((p) => p.id)).toEqual(['p2', 'p3', 'p4'])
    await b.lecteur.retirer('e2')
    expect(b.lecteur.aSuivre.map((p) => p.id)).toEqual(['p3', 'p4'])
    srv.etat.f = { ...srv.etat.f, items: [...srv.etat.f.items, srv.el(piste(1, { id: 'p9' }), true, true)] }
    await b.lecteur.reglerSuggestions('frequent')
    expect(b.lecteur.suggestions).toBe('frequent')
    expect(b.lecteur.elements.at(-1)).toMatchObject({ suggested: true, discovery: true })
    expect(b.audios[0].lectures).toBe(1)
  })
})

describe('répétition, fenêtre de suggestions, titres similaires', () => {
  it('répétition : aucune, file, titre ; envoyée au serveur et suivie en fin de piste et de file', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur })
    b.lecteur.jouer([piste(1), piste(2)], 1)
    await attendre(() => b.lecteur.elements.length === 2 && b.audios[0]?.lectures === 1)
    expect(b.lecteur.repetition).toBe('off')
    expect(b.lecteur.peutSuivre).toBe(false)
    expect(b.lecteur.suivante).toBeNull()

    await b.lecteur.changerRepetition()
    expect(srv.appels.at(-1)).toEqual(['PATCH', { revision: 1, repeat: 'all' }])
    expect(b.lecteur.repetition).toBe('all')
    expect(b.lecteur.peutSuivre).toBe(true)
    expect(b.lecteur.suivante?.id).toBe('p1')
    const a = b.audios[0]
    a.emettre('playing')
    await attendre(() => b.audios[1].src.endsWith('/p1'))
    a.emettre('ended')
    await attendre(() => b.lecteur.index === 0 && b.audios[1].lectures === 1)
    await attendre(() => srv.etat.f.index === 0)
    expect(srv.appels.at(-1)).toEqual(['PATCH', { revision: 2, index: 0 }])

    await b.lecteur.changerRepetition()
    expect(b.lecteur.repetition).toBe('one')
    expect(b.lecteur.suivante?.id).toBe('p1')
    const c = b.audios[1]
    c.emettre('playing')
    await attendre(() => b.audios[0].src.endsWith('/p1'))
    c.emettre('ended')
    await attendre(() => b.audios[0].lectures === 2)
    expect(b.lecteur.index).toBe(0)
    expect(b.lecteur.piste?.id).toBe('p1')

    b.lecteur.suivant()
    await attendre(() => b.lecteur.index === 1)
    await b.lecteur.changerRepetition()
    expect(srv.appels.filter(([, c]) => c.repeat).map(([, c]) => c.repeat)).toEqual(['all', 'one', 'off'])
    expect(b.lecteur.peutSuivre).toBe(false)
  })

  it('changement de piste : index envoyé, suggestions ajoutées par le serveur rechargées et préchargées', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur })
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.lecteur.elements.length === 3 && b.audios[0]?.lectures === 1)
    srv.etat.planifier = [srv.el(piste(4), true), srv.el(piste(9, { id: 'p9' }), true, true)]
    b.lecteur.suivant()
    await attendre(() => b.lecteur.elements.length === 5)
    expect(srv.appels.at(-1)).toEqual(['PATCH', { revision: 1, index: 1 }])
    expect(b.lecteur.aSuivre.map((p) => p.id)).toEqual(['p4', 'p9', 'p3'])
    expect(b.lecteur.elements.slice(2, 4).map(marqueDe)).toEqual(['Suggestion', 'Découverte'])
    await attendre(() => b.audios[0]?.src.endsWith('/p2'))
    b.audios[0].emettre('playing')
    await attendre(() => b.audios[1].src.endsWith('/p4'))
  })

  it('« ne plus proposer » : refus envoyé puis élément retiré de la file en cours', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur })
    const refus = vi.spyOn(sillonApi, 'nePlusProposer').mockResolvedValue(undefined)
    b.lecteur.jouer(album, 0, { type: 'album', id: 'a1' })
    await attendre(() => b.lecteur.elements.length === 3)
    await b.lecteur.nePlusProposer(b.lecteur.elements[2])
    expect(refus).toHaveBeenCalledWith('p3')
    expect(srv.appels.at(-1)).toEqual(['POST /queue/remove', { revision: 1, items: ['e3'] }])
    expect(b.lecteur.aSuivre.map((p) => p.id)).toEqual(['p2'])
  })

  it('titres similaires : les pistes proches remplacent la file et se jouent', async () => {
    const srv = fauxServeur(catalogue)
    const b = banc({ serveur: srv.serveur })
    const proches = vi.spyOn(sillonApi, 'similaires').mockResolvedValue({ items: [piste(3), piste(4)] })
    expect(await b.lecteur.similaires(piste(1))).toBe(2)
    expect(proches).toHaveBeenCalledWith('p1')
    await attendre(() => b.audios[0]?.lectures === 1)
    expect(b.audios[0].src).toBe('https://sillon.test/stream/p3')
    expect(srv.appels[0]).toEqual(['PUT', { revision: 0, tracks: ['p3', 'p4'], index: 0 }])
    proches.mockResolvedValue({ items: [] })
    expect(await b.lecteur.similaires(piste(1))).toBe(0)
  })

  it('titres similaires : GET /tracks/{id}/similar avec la limite', async () => {
    const f = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{"items":[]}', { headers: { 'Content-Type': 'application/json' } }))
    await sillonApi.similaires('p1', 10)
    expect(String(f.mock.calls[0][0])).toMatch(/\/tracks\/p1\/similar\?limit=10$/)
  })
})

describe('playlists, ajouts et administration', () => {
  it('déplacement dans une liste', () => {
    expect(deplacerListe([1, 2, 3, 4], 0, 2)).toEqual([2, 3, 1, 4])
    expect(deplacerListe([1, 2, 3, 4], 3, 0)).toEqual([4, 1, 2, 3])
    expect(deplacerListe([1, 2], 0, 5)).toEqual([1, 2])
  })

  it('tailles et refus lisibles : disque plein avec l’occupation, limite du jour, déjà présent', () => {
    expect(octets(512)).toBe('512 o')
    expect(octets(1_500_000_000)).toBe('1,5 Go')
    expect(octets(200_000_000_000)).toBe('200 Go')
    const plein = new ApiError(507, 'disk_full', 'le disque de la musique serait plein', { used: 480e9, pending: 10e9, limit: 500e9, free: 10e9, estimated: 2e10 })
    expect(refusAjout(plein)).toBe('Disque plein : 490 Go sur 500 Go occupés, 10 Go libres ; cet ajout demande environ 20 Go.')
    expect(refusAjout(new ApiError(429, 'daily_limit', 'limite de 5 ajouts par jour atteinte'))).toBe('Limite d’ajouts atteinte : limite de 5 ajouts par jour atteinte.')
    expect(refusAjout(new ApiError(409, 'already_available', 'x'))).toBe('Déjà dans la bibliothèque.')
    expect(refusAjout(new ApiError(409, 'excluded', 'x'))).toMatch(/administrateur/)
  })

  it('le message d’erreur de l’API garde son corps (507, 409 de la file)', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ error: 'disk_full', message: 'plein', used: 1, pending: 0, limit: 2, free: 1, estimated: 5 }), { status: 507, headers: { 'Content-Type': 'application/json' } }))
    const e = await sillonApi.ajouter('album', 'm').catch((x) => x)
    expect(e).toBeInstanceOf(ApiError)
    expect(e.data).toMatchObject({ used: 1, estimated: 5 })
  })

  it('budget de la découverte atteint (507 discovery_budget) : message propre, sans occupation', () => {
    expect(refusAjout(new ApiError(507, 'discovery_budget', 'budget de découverte atteint'))).toBe('Budget de la découverte atteint.')
  })

  it('marque d’un élément de file : découverte distincte de suggestion', () => {
    expect(marqueDe({ suggested: true, discovery: false })).toBe('Suggestion')
    expect(marqueDe({ suggested: true, discovery: true })).toBe('Découverte')
    expect(marqueDe({ suggested: false, discovery: true })).toBe('Découverte')
    expect(marqueDe({ suggested: false, discovery: false })).toBeNull()
    expect(marqueDe(null)).toBeNull()
  })

  it('source de la file selon le contexte, et retour', () => {
    expect(sourceDe({ type: 'playlist', id: 'x' })).toEqual({ type: 'playlist', id: 'x' })
    expect(sourceDe({ type: 'likes' })).toEqual({ type: 'likes' })
    expect(sourceDe({ type: 'pistes' })).toBeUndefined()
    expect(sourceDe({ type: 'pistes', query: 'daft' })).toEqual({ type: 'search', query: 'daft' })
    expect(contexteDe({ type: 'search', id: null, name: 'daft' })).toEqual({ type: 'pistes', query: 'daft' })
    expect(contexteDe({ type: 'playlist', id: 'x', name: 'P' })).toEqual({ type: 'playlist', id: 'x' })
    expect(contexteDe(null)).toEqual({ type: 'pistes' })
  })
})
