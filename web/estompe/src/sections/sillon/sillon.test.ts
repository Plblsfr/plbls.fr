import { afterEach, describe, expect, it, vi } from 'vitest'
import { medias } from '../../lib/medias'
import { Lecteur, type ContexteAudio, type DetailsAction, type ElementAudio, type Environnement, type NoeudGain } from './sillon.svelte'
import { anneeEtType, debut, dureeExacte, gainLineaire, sections, sillonApi, support, totalAlbum, type Lecture, type Piste } from './sillon'

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
    vi.spyOn(sillonApi, 'qualite').mockResolvedValue({ quality: 'basse', qualities: [] })
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

  it('reprise : la file et la position sont gardées et relancées', async () => {
    const b = banc()
    b.lecteur.jouer(album, 1, { type: 'album', id: 'a1' })
    await attendre(() => b.lecteur.lecture)
    b.lecteur.position = 63
    b.lecteur.pause()
    const sauve = JSON.parse(b.stock.get('sillon.reprise')!)
    expect(sauve.index).toBe(1)
    expect(sauve.position).toBe(63)
    const c = banc({ stockage: { getItem: (k) => b.stock.get(k) ?? null, setItem: () => {} } })
    expect(c.lecteur.reprise?.file[1].id).toBe('p2')
    c.lecteur.reprendreSauvegarde()
    await attendre(() => c.audios[0]?.lectures === 1)
    expect(c.lecteur.piste?.id).toBe('p2')
    expect(c.audios[0].currentTime).toBe(63)
  })
})
