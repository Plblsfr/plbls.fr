import { describe, expect, it } from 'vitest'
import { absolute, bufferedAhead, clock, inIntro, mergeRanges, seekTrack, isForced, pickAudio, pickSubtitle, playPrefs, savePlayPrefs, effectiveBuffer, refuseCodec, refusedCodecs, shortcut, needsReopen, shiftCues, savedQuality, saveQuality, support, episodeAfter, episodeLabel, episodesLabel, dayLabel, historyDays, fullTitle, nearEnd, seasonAfter, percent, pickEpisode, reasonLabel, remaining, resumeAt, saveDue, shortDuration, tint, trackName, yearOptions, type FilmItem } from './films'

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
  it('épisode suivant : le premier disponible après celui en cours, puis la saison suivante', () => {
    const liste = [item({ id: '1' }), item({ id: '2', available: false }), item({ id: '3' })]
    expect(episodeAfter(liste, '1')?.id).toBe('3')
    expect(episodeAfter(liste, '3')).toBeUndefined()
    expect(episodeAfter(liste, 'absent')).toBeUndefined()
    const saisons = [item({ id: 's·3', kind: 'season', season: 3 }), item({ id: 's·0', kind: 'season', season: 0 }), item({ id: 's·2', kind: 'season', season: 2, available: false }), item({ id: 's·1', kind: 'season', season: 1 })]
    expect(seasonAfter(saisons, 1)?.id).toBe('s·3')
    expect(seasonAfter(saisons, 0)?.id).toBe('s·1')
    expect(seasonAfter(saisons, 3)).toBeUndefined()
  })
  it('fin d’un titre : les trente dernières secondes', () => {
    expect(nearEnd(2969, 3000)).toBe(false)
    expect(nearEnd(2970, 3000)).toBe(true)
    expect(nearEnd(10, 0)).toBe(false)
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
    const defauts = { audio: 'auto', subtitles: 'none', volume: 100, muted: false, buffer: 'normal', wifiOnly: false }
    expect(playPrefs(null)).toEqual(defauts)
    expect(playPrefs(memoire({ 'entracte.lecture': 'pas du json' }))).toEqual(defauts)
    expect(playPrefs(memoire({ 'entracte.lecture': '{"audio":"klingon","subtitles":7,"volume":"fort","muted":"oui","buffer":"infini","wifiOnly":"oui"}' }))).toEqual(defauts)
    expect(playPrefs(memoire({ 'entracte.lecture': '{"volume":250}' })).volume).toBe(100)
  })

  it('garde le volume et les choix d’une lecture à l’autre', () => {
    const m = memoire()
    savePlayPrefs(m, { volume: 35, muted: true })
    savePlayPrefs(m, { audio: 'vff', subtitles: 'forced', buffer: 'all', wifiOnly: true })
    expect(playPrefs(m)).toEqual({ audio: 'vff', subtitles: 'forced', volume: 35, muted: true, buffer: 'all', wifiOnly: true })
  })

  it('ramène le préchargement au réglage normal en données mobiles quand il est réservé au Wi-Fi', () => {
    expect(effectiveBuffer({ buffer: 'all', wifiOnly: true }, 'cellular')).toBe('normal')
    expect(effectiveBuffer({ buffer: 'all', wifiOnly: true }, 'wifi')).toBe('all')
    expect(effectiveBuffer({ buffer: 'long', wifiOnly: true }, 'unknown')).toBe('long')
    expect(effectiveBuffer({ buffer: 'long', wifiOnly: false }, 'cellular')).toBe('long')
  })

  it('n’annonce plus un codec que l’appareil n’a pas su lire', () => {
    const m = memoire()
    expect(refusedCodecs(m)).toEqual([])
    expect(refusedCodecs(memoire({ 'entracte.refus': '{"hevc":1}' }))).toEqual([])
    expect(support(() => 'probably').video).toEqual(['h264', 'hevc', 'hevc10', 'vp9', 'av1'])
    refuseCodec(m, 'hevc')
    refuseCodec(m, 'hevc')
    expect(refusedCodecs(m)).toEqual(['hevc'])
    expect(support(() => 'probably', refusedCodecs(m)).video).toEqual(['h264', 'vp9', 'av1'])
    expect(support(() => 'probably', refusedCodecs(m)).audio).toContain('aac')
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
    expect([' ', 'k', 'K', 'ArrowLeft', 'j', 'ArrowRight', 'l', 'ArrowUp', 'ArrowDown', 'm', 'f', 'F', 's', 'S'].map((k) => touche(k)))
      .toEqual(['toggle', 'toggle', 'toggle', 'back', 'back', 'forward', 'forward', 'louder', 'quieter', 'mute', 'fullscreen', 'fullscreen', 'skip', 'skip'])
  })

  it('situe la position par rapport à l’intro', () => {
    const intro = { debut: 30, fin: 90 }
    expect([29.9, 30, 60, 89.9, 90, 200].map((t) => inIntro(t, intro))).toEqual([false, true, true, true, false, false])
    expect(inIntro(40, null)).toBe(false)
    expect(inIntro(40, undefined)).toBe(false)
    expect(inIntro(40, { debut: 50, fin: 50 })).toBe(false)
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

describe('historique', () => {
  const maintenant = new Date(2026, 10, 25, 22, 0)
  const le = (jour: number, heure: number) => new Date(2026, 10, jour, heure, 0).toISOString()

  it('libellé du jour', () => {
    expect(dayLabel(new Date(2026, 10, 25, 0, 5), maintenant)).toBe('Aujourd’hui')
    expect(dayLabel(new Date(2026, 10, 24, 23, 59), maintenant)).toBe('Hier')
    expect(dayLabel(new Date(2026, 10, 23, 12, 0), maintenant)).toBe('Lundi 23 novembre')
    expect(dayLabel(new Date(2025, 11, 31, 12, 0), maintenant)).toBe('Mercredi 31 décembre 2025')
  })

  it('épisodes en plages', () => {
    const ep = (season: number, episode: number) => ({ season, episode })
    expect(episodesLabel([ep(1, 8), ep(1, 7), ep(1, 6), ep(1, 5)])).toBe('S1 · E5 à E8')
    expect(episodesLabel([ep(1, 10), ep(1, 9)])).toBe('S1 · E9, E10')
    expect(episodesLabel([ep(1, 3), ep(1, 5), ep(1, 6), ep(1, 7), ep(1, 3)])).toBe('S1 · E3, E5 à E7')
    expect(episodesLabel([ep(2, 1), ep(1, 13)])).toBe('S1 · E13 ; S2 · E1')
    expect(episodesLabel([{ season: null, episode: null }])).toBe('')
  })

  it('regroupe par jour puis par série', () => {
    const jours = historyDays([
      item({ id: 'p9', series_id: 'pun', episode: 9, seen_at: le(25, 21) }),
      item({ id: 'd10', series_id: 'dar', episode: 10, seen_at: le(25, 20) }),
      item({ id: 'p8', series_id: 'pun', episode: 8, seen_at: le(25, 19) }),
      item({ id: 'f', kind: 'movie', series_id: null, series_title: null, season: null, episode: null, seen_at: le(25, 15) }),
      item({ id: 'p7', series_id: 'pun', episode: 7, seen_at: le(24, 21) }),
      item({ id: 'sans-date', seen_at: null }),
      item({ id: 'illisible', seen_at: 'pas une date' }),
    ], maintenant)
    expect(jours.map((j) => [j.label, j.entries.map((e) => [e.item.id, e.episodes.map((x) => x.id)])])).toEqual([
      ['Aujourd’hui', [['p9', ['p9', 'p8']], ['d10', ['d10']], ['f', ['f']]]],
      ['Hier', [['p7', ['p7']]]],
    ])
    expect(jours[0].entries[0].seen_at).toBe(le(25, 21))
    expect(historyDays([], maintenant)).toEqual([])
  })
})
