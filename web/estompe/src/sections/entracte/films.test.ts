import { describe, expect, it } from 'vitest'
import { absolute, bufferedAhead, clock, mergeRanges, seekTrack, isForced, pickAudio, pickSubtitle, playPrefs, savePlayPrefs, shortcut, needsReopen, shiftCues, savedQuality, saveQuality, support, episodeLabel, fullTitle, percent, pickEpisode, reasonLabel, remaining, resumeAt, saveDue, shortDuration, tint, trackName, yearOptions, type FilmItem } from './films'

const item = (over: Partial<FilmItem> = {}): FilmItem => ({
  id: 'a', kind: 'episode', title: 'La Combe', year: 2024, overview: null, duration: 3000, genres: [], season: 2, episode: 4,
  series_id: 's', series_title: 'Les Veilleurs', poster: false, backdrop: false, progress: null, ...over,
})
const seen = { position: 2900, duration: 3000, seen: true }
const started = { position: 600, duration: 3000, seen: false }

describe('formatage', () => {
  it('horloge', () => {
    expect(clock(0)).toBe('0:00')
    expect(clock(65)).toBe('1:05')
    expect(clock(3972)).toBe('1:06:12')
    expect(clock(-3)).toBe('0:00')
  })
  it('durée courte et reste', () => {
    expect(shortDuration(2880)).toBe('48 min')
    expect(shortDuration(6720)).toBe('1 h 52')
    expect(remaining(started)).toBe('reste 40 min')
    expect(remaining({ position: 4000, duration: 3000, seen: true })).toBe('reste 0 min')
  })
  it('pourcentage borné', () => {
    expect(percent(null)).toBe('0%')
    expect(percent(started)).toBe('20%')
    expect(percent({ position: 9, duration: 3, seen: true })).toBe('100%')
    expect(percent({ position: 9, duration: 0, seen: false })).toBe('0%')
  })
  it('titres', () => {
    expect(episodeLabel(item())).toBe('S2 · E4')
    expect(fullTitle(item())).toBe('Les Veilleurs · S2 · E4, La Combe')
    expect(fullTitle(item({ series_title: null, season: null, episode: null }))).toBe('La Combe')
  })
  it('noms de pistes et raisons', () => {
    expect(trackName({ n: 0, label: 'Français', language: 'fra' })).toBe('Français')
    expect(trackName({ n: 0, label: null, language: 'fra' })).toBe('fra')
    expect(trackName({ n: 1, label: null, language: null })).toBe('Piste 2')
    expect(reasonLabel('double_extension')).toBe('Double extension')
    expect(reasonLabel('inconnue')).toBe('inconnue')
  })
  it('filtre des années', () => {
    const o = yearOptions(2026)
    expect(o[0].value).toBe('')
    expect(o[1].value).toBe('2020-2026')
    expect(yearOptions(2019)[1].value).toBe('2020-2020')
  })
  it('couleur stable par titre', () => {
    expect(tint('abc')).toEqual(tint('abc'))
    expect(tint('')).toBeDefined()
  })
})

describe('progression', () => {
  it('enregistre toutes les 15 secondes', () => {
    expect(saveDue(0, 14_999)).toBe(false)
    expect(saveDue(0, 15_000)).toBe(true)
    expect(saveDue(100_000, 110_000)).toBe(false)
  })
  it('reprend une lecture commencée, pas une lecture vue', () => {
    expect(resumeAt(item({ progress: started }))).toBe(600)
    expect(resumeAt(item({ progress: seen }))).toBe(0)
    expect(resumeAt(item())).toBe(0)
  })
  it('choisit l\'épisode en cours, puis le premier non vu, puis le premier', () => {
    const e1 = item({ id: '1', progress: seen })
    const e2 = item({ id: '2' })
    const e3 = item({ id: '3', progress: started })
    expect(pickEpisode([e1, e2, e3])?.id).toBe('3')
    expect(pickEpisode([e1, e2])?.id).toBe('2')
    expect(pickEpisode([e1])?.id).toBe('1')
    expect(pickEpisode([])).toBeUndefined()
  })
})

describe('Entracte : formats, qualité et position', () => {
  it('annonce ce que le lecteur sait lire', () => {
    const ok = new Set(['video/mp4', 'video/webm', 'video/mp4; codecs="avc1.42E01E"', 'video/webm; codecs="vp9"', 'audio/mp4; codecs="mp4a.40.2"', 'audio/webm; codecs="opus"'])
    expect(support((m) => (ok.has(m) ? 'probably' : ''))).toEqual({ containers: ['mp4', 'webm'], video: ['h264', 'vp9'], audio: ['aac', 'opus'] })
    expect(support(() => '')).toEqual({ containers: [], video: [], audio: [] })
  })
  it('palier mémorisé par appareil, repli sur Originale', () => {
    const m = new Map<string, string>()
    const store = { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) }
    expect(savedQuality(store)).toBe('original')
    saveQuality(store, '720p')
    expect(savedQuality(store)).toBe('720p')
    expect(savedQuality(store, ['original', '480p'])).toBe('original')
    m.set('entracte.qualite', '4k')
    expect(savedQuality(store)).toBe('original')
    expect(savedQuality(null)).toBe('original')
    const casse = { getItem: () => { throw new Error('bloqué') }, setItem: () => { throw new Error('bloqué') } }
    expect(savedQuality(casse)).toBe('original')
    expect(() => saveQuality(casse, '480p')).not.toThrow()
  })
  it('position réelle d’un flux HLS qui commence plus loin', () => {
    expect(absolute(10, 600)).toBe(610)
    expect(needsReopen(550, 600, 120)).toBe(true)
    expect(needsReopen(650, 600, 120)).toBe(false)
    expect(needsReopen(900, 600, 120)).toBe(true)
  })
})

describe('Entracte : sous-titres d’un flux qui ne commence pas à zéro', () => {
  it('ramène les répliques au temps du lecteur', () => {
    const cues = [{ startTime: 590, endTime: 602 }, { startTime: 660, endTime: 663.5 }]
    shiftCues(cues, 600)
    expect(cues).toEqual([{ startTime: -10, endTime: 2 }, { startTime: 60, endTime: 63.5 }])
  })

  it('ne touche à rien en lecture directe ou sans réplique', () => {
    const cues = [{ startTime: 5, endTime: 8 }]
    shiftCues(cues, 0)
    shiftCues(null, 600)
    expect(cues).toEqual([{ startTime: 5, endTime: 8 }])
  })
})

const piste = (n: number, language: string | null, label: string | null = null, forced?: boolean) => ({ n, language, label, forced })
const memoire = (depart: Record<string, string> = {}) => {
  const m = new Map(Object.entries(depart))
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => { m.set(k, v) } }
}

describe('préférences de lecture', () => {
  it('part des valeurs par défaut et ignore un contenu invalide', () => {
    const defauts = { audio: 'auto', subtitles: 'none', volume: 100, muted: false }
    expect(playPrefs(null)).toEqual(defauts)
    expect(playPrefs(memoire({ 'entracte.lecture': 'pas du json' }))).toEqual(defauts)
    expect(playPrefs(memoire({ 'entracte.lecture': '{"audio":"klingon","subtitles":7,"volume":"fort","muted":"oui"}' }))).toEqual(defauts)
    expect(playPrefs(memoire({ 'entracte.lecture': '{"volume":250}' })).volume).toBe(100)
  })

  it('garde le volume et les choix d’une lecture à l’autre', () => {
    const m = memoire()
    savePlayPrefs(m, { volume: 35, muted: true })
    savePlayPrefs(m, { audio: 'vff', subtitles: 'forced' })
    expect(playPrefs(m)).toEqual({ audio: 'vff', subtitles: 'forced', volume: 35, muted: true })
  })
})

describe('piste audio par défaut', () => {
  const pistes = [piste(0, 'eng', 'English (EAC3, 5.1)'), piste(1, 'fre', 'VFQ (AC3, 5.1)'), piste(2, 'fre', 'VFF (AC3, 5.1)'), piste(3, 'fre', 'VFF Audiodescription (AAC, stéréo)')]

  it('garde la première piste du fichier sans préférence', () => {
    expect(pickAudio(pistes, 'auto')).toBe(0)
  })

  it('choisit la version française demandée', () => {
    expect(pickAudio(pistes, 'vff')).toBe(2)
    expect(pickAudio(pistes, 'vfq')).toBe(1)
    expect(pickAudio(pistes, 'vf')).toBe(1)
    expect(pickAudio(pistes, 'en')).toBe(0)
  })

  it('se rabat sur une autre version française, puis sur la première piste', () => {
    expect(pickAudio([piste(0, 'eng'), piste(1, 'fre', 'VFQ')], 'vff')).toBe(1)
    expect(pickAudio([piste(0, 'jpn'), piste(1, 'eng')], 'vff')).toBe(0)
    expect(pickAudio([], 'vf')).toBe(0)
  })

  it('reconnaît la langue dans le libellé quand le fichier ne la donne pas', () => {
    expect(pickAudio([piste(0, null, 'English'), piste(1, null, 'TrueFrench 5.1')], 'vff')).toBe(1)
    expect(pickAudio([piste(0, null, 'Piste (AAC, stéréo)'), piste(1, 'fr-CA')], 'vfq')).toBe(1)
  })

  it('évite l’audiodescription quand une autre piste convient', () => {
    expect(pickAudio([piste(0, 'eng'), piste(1, 'fre', 'VFF Audiodescription'), piste(2, 'fre', 'Français')], 'vff')).toBe(2)
  })
})

describe('sous-titres par défaut', () => {
  const pistes = [piste(0, 'eng', 'English'), piste(1, 'fre', 'Français forcés'), piste(2, 'fre', 'Français SDH'), piste(3, 'fre', 'Français'), piste(4, 'eng', 'Signs', true)]

  it('reconnaît une piste forcée par son marqueur ou son libellé', () => {
    expect(pistes.map(isForced)).toEqual([false, true, false, false, true])
  })

  it('n’affiche rien sans préférence', () => {
    expect(pickSubtitle(pistes, 'none', piste(0, 'fre'))).toBeNull()
  })

  it('prend les sous-titres forcés de la langue de la piste audio', () => {
    expect(pickSubtitle(pistes, 'forced', piste(0, 'fre', 'VFF'))).toBe(1)
    expect(pickSubtitle(pistes, 'forced', piste(0, 'eng'))).toBe(4)
    expect(pickSubtitle(pistes, 'forced', piste(0, 'jpn'))).toBeNull()
    expect(pickSubtitle(pistes, 'forced')).toBe(1)
    expect(pickSubtitle([piste(0, 'fre', 'Français')], 'forced', piste(0, 'fre'))).toBeNull()
  })

  it('prend les sous-titres complets de la langue demandée, les forcés à défaut', () => {
    expect(pickSubtitle(pistes, 'fr')).toBe(3)
    expect(pickSubtitle(pistes, 'en')).toBe(0)
    expect(pickSubtitle([piste(0, 'fre', null, true)], 'fr')).toBe(0)
    expect(pickSubtitle([piste(0, 'eng')], 'fr')).toBeNull()
  })
})

describe('raccourcis clavier du lecteur', () => {
  const touche = (key: string, over: Partial<KeyboardEvent> = {}) => shortcut({ key, ctrlKey: false, metaKey: false, altKey: false, ...over })

  it('associe les touches aux commandes', () => {
    expect([' ', 'k', 'K', 'ArrowLeft', 'j', 'ArrowRight', 'l', 'ArrowUp', 'ArrowDown', 'm', 'f', 'F'].map((k) => touche(k)))
      .toEqual(['toggle', 'toggle', 'toggle', 'back', 'back', 'forward', 'forward', 'louder', 'quieter', 'mute', 'fullscreen', 'fullscreen'])
  })

  it('laisse passer les autres touches et les combinaisons', () => {
    expect(touche('a')).toBeNull()
    expect(touche('Enter')).toBeNull()
    expect(touche('k', { ctrlKey: true })).toBeNull()
    expect(touche('f', { metaKey: true })).toBeNull()
    expect(touche('ArrowLeft', { altKey: true })).toBeNull()
  })
})

describe('préchargement sur la barre de position', () => {
  const VIDE = 'rgb(255 255 255 / 22%)'
  const CHARGE = 'rgb(255 255 255 / 52%)'

  it('réunit les plages qui se touchent et écarte les plages vides', () => {
    expect(mergeRanges([[40, 60], [0, 10], [10.2, 20], [30, 30], [Number.NaN, 5]])).toEqual([[0, 20], [40, 60]])
  })

  it('peint la partie lue, puis ce qui est chargé après elle, puis le reste', () => {
    expect(seekTrack(50, 200, [[40, 100]])).toBe(`linear-gradient(to right, var(--rose-fonce) 0 25%, ${CHARGE} 25% 50%, ${VIDE} 50% 100%)`)
  })

  it('montre une plage chargée plus loin, séparée de la position', () => {
    expect(seekTrack(20, 200, [[0, 40], [100, 150]])).toBe(`linear-gradient(to right, var(--rose-fonce) 0 10%, ${CHARGE} 10% 20%, ${VIDE} 20% 50%, ${CHARGE} 50% 75%, ${VIDE} 75% 100%)`)
  })

  it('ne dépasse pas la durée et reste vide sans durée connue', () => {
    expect(seekTrack(150, 200, [[100, 260]])).toBe(`linear-gradient(to right, var(--rose-fonce) 0 75%, ${CHARGE} 75% 100%)`)
    expect(seekTrack(10, 0, [[0, 30]])).toBe(`linear-gradient(to right, ${VIDE} 0 100%)`)
  })

  it('compte la durée chargée après la position, dans la plage qui la contient', () => {
    expect(bufferedAhead(50, [[40, 100], [100.3, 130], [300, 400]])).toBe(80)
    expect(bufferedAhead(200, [[40, 100], [300, 400]])).toBe(0)
  })
})
