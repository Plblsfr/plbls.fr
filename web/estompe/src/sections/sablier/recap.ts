import { deMois, duree, dureeRonde, enMois, jourDeSemaine, jourEtMois, MOIS, nombre, pluriel, versDate } from './formats'
import { libelleRecap } from './periodes'
import type { Fait, Recap, RecapResume, TropheePublic } from './sablier'
import { salonTexte, salonVocal, type NomSalon } from './stats'

export interface Morceau { texte: string; fort?: boolean }
export interface Ecran {
  cle: 'ouverture' | 'usage' | 'entracte' | 'sillon' | 'messaging' | 'trophees'
  surtitre: string
  grand: Morceau[]
  titre: Morceau[]
  texte: string
  trophees: TropheePublic[]
  reste: number
}

const MEDAILLES = 4
const NOM_PLATEFORME: Record<string, string> = { web: 'le web', pwa: 'l’app web', windows: 'Windows', android: 'Android' }

const ecran = (cle: Ecran['cle'], surtitre: string, grand: Morceau[], titre: Morceau[], phrases: string[]): Ecran => ({ cle, surtitre, grand, titre, texte: phrases.filter(Boolean).join(' '), trophees: [], reste: 0 })
const fait = (r: Recap, genre: string): Fait | undefined => r.highlights.find((h) => h.kind === genre && h.value > 0)
const quand = (r: Recap, jour: string) => (r.kind === 'year' ? `le ${jourEtMois(jour)}` : jourDeSemaine(jour))

function moisLePlusRempli(r: Recap): string {
  const parMois = new Map<number, number>()
  for (const j of r.usage.by_day) parMois.set(versDate(j.day).getUTCMonth(), (parMois.get(versDate(j.day).getUTCMonth()) ?? 0) + j.seconds)
  if (parMois.size < 2) return ''
  const [mois] = [...parMois.entries()].sort((a, b) => b[1] - a[1])[0]
  return `Ton mois le plus rempli : ${MOIS[mois]}.`
}

function usage(r: Recap): Ecran | null {
  const u = r.usage
  if (u.seconds < 60) return null
  const titre: Morceau[] = u.active_days >= 2
    ? [{ texte: 'de temps actif, sur ' }, { texte: pluriel(u.active_days, 'jour', 'jours'), fort: true }, { texte: r.kind === 'year' ? ' de l’année.' : ' du mois.' }]
    : [{ texte: 'de ' }, { texte: 'temps actif', fort: true }, { texte: '.' }]
  const longue = fait(r, 'longest_day')
  const plateforme = u.by_platform.length >= 2 && u.by_platform[0].seconds > 0 ? `Surtout sur ${NOM_PLATEFORME[u.by_platform[0].platform] ?? u.by_platform[0].platform}.` : ''
  return ecran('usage', 'Dans l’app', [{ texte: dureeRonde(u.seconds) }], titre, [
    longue?.day && u.active_days >= 2 ? `Ta plus longue journée : ${quand(r, longue.day)}, avec ${duree(longue.value)}.` : '',
    r.kind === 'year' ? moisLePlusRempli(r) : plateforme,
  ])
}

function entracte(r: Recap): Ecran | null {
  const e = r.entracte
  if (!e) return null
  const comptes = [
    { n: e.episodes_watched, texte: pluriel(e.episodes_watched, 'épisode', 'épisodes') },
    { n: e.movies_watched, texte: pluriel(e.movies_watched, 'film', 'films') },
  ].filter((c) => c.n > 0).sort((a, b) => b.n - a.n)
  if (!comptes.length && e.seconds < 60) return null
  const series: Morceau[] = e.series_completed > 0 ? [{ texte: 'Tu as terminé ' }, { texte: pluriel(e.series_completed, 'série', 'séries'), fort: true }, { texte: '.' }] : []
  let grand: string
  let titre: Morceau[]
  if (!comptes.length) {
    grand = dureeRonde(e.seconds)
    titre = [{ texte: 'passées devant ' }, { texte: 'Entracte', fort: true }, { texte: '.' }]
  } else {
    grand = comptes[0].texte
    if (comptes[1]) titre = [{ texte: `et ${comptes[1].texte}. ` }, ...series]
    else if (series.length) titre = series
    else titre = e.seconds >= 60 ? [{ texte: 'en ' }, { texte: dureeRonde(e.seconds), fort: true }, { texte: ' devant Entracte.' }] : [{ texte: 'vus ' }, { texte: r.kind === 'year' ? 'cette année' : 'ce mois-ci', fort: true }, { texte: '.' }]
  }
  const grosse = fait(r, 'biggest_day')
  const enchaines = fait(r, 'episodes_in_a_row')
  return ecran('entracte', 'Entracte', [{ texte: grand }], titre, [
    enchaines && enchaines.value >= 2 ? `Tu as enchaîné ${nombre(enchaines.value)} épisodes d’un coup.` : '',
    grosse?.day && grosse.value < e.seconds ? `Ta plus grosse journée : ${duree(grosse.value)}, ${quand(r, grosse.day)}.` : '',
    e.cinema_sessions > 0 ? `${pluriel(e.cinema_sessions, 'séance', 'séances')} à plusieurs au salon cinéma.` : '',
  ])
}

function sillon(r: Recap): Ecran | null {
  const s = r.sillon
  if (!s || (s.seconds < 60 && s.tracks_played <= 0)) return null
  if (s.seconds < 60) return ecran('sillon', 'Sillon', [{ texte: pluriel(s.tracks_played, 'titre', 'titres') }], [{ texte: s.tracks_played >= 2 ? 'écoutés dans ' : 'écouté dans ' }, { texte: 'Sillon', fort: true }, { texte: '.' }], [])
  const titre: Morceau[] = s.distinct_tracks > 0
    ? [{ texte: 'd’écoute, pour ' }, { texte: pluriel(s.distinct_tracks, 'titre différent', 'titres différents'), fort: true }, { texte: '.' }]
    : [{ texte: 'd’' }, { texte: 'écoute', fort: true }, { texte: '.' }]
  return ecran('sillon', 'Sillon', [{ texte: dureeRonde(s.seconds) }], titre, [
    s.tracks_played > s.distinct_tracks ? `${pluriel(s.tracks_played, 'titre écouté', 'titres écoutés')} en tout${r.kind === 'year' ? ' sur l’année' : ''}.` : '',
  ])
}

function messagerie(r: Recap, nom: NomSalon): Ecran | null {
  const m = r.messaging
  if (m.total <= 0 && m.voice_seconds < 60) return null
  const vocal = m.voice_seconds >= 60
  if (m.total <= 0) {
    const prefere = fait(r, 'top_voice_channel')
    return ecran('messaging', 'Messagerie et vocal', [{ texte: dureeRonde(m.voice_seconds) }], [{ texte: 'passées ' }, { texte: 'en vocal', fort: true }, { texte: '.' }], [
      prefere?.server && prefere.channel && nom(prefere.server, prefere.channel) ? `Surtout dans le ${salonVocal({ server: prefere.server, channel: prefere.channel }, nom)}.` : '',
      m.longest_call > 0 ? `Ton plus long appel : ${duree(m.longest_call)}.` : '',
    ])
  }
  const bavarde = fait(r, 'busiest_day')
  let titre: Morceau[]
  if (bavarde?.day && bavarde.value < m.total) titre = [{ texte: `dont ${nombre(bavarde.value)} ` }, { texte: quand(r, bavarde.day), fort: true }, { texte: ', ta journée la plus bavarde.' }]
  else if (vocal) titre = [{ texte: 'et ' }, { texte: `${dureeRonde(m.voice_seconds)} en vocal`, fort: true }, { texte: '.' }]
  else titre = [{ texte: m.total >= 2 ? 'envoyés ' : 'envoyé ' }, { texte: r.kind === 'year' ? 'cette année' : 'ce mois-ci', fort: true }, { texte: '.' }]
  const salon = fait(r, 'top_channel')
  const emoji = fait(r, 'top_emoji')
  const nomDuSalon = salon?.server && salon.channel && nom(salon.server, salon.channel) ? salonTexte({ server: salon.server, channel: salon.channel }, nom) : ''
  return ecran('messaging', 'Messagerie et vocal', [{ texte: pluriel(m.total, 'message', 'messages') }], titre, [
    nomDuSalon ? `${nomDuSalon} reste ton salon.` : '',
    vocal ? `${duree(m.voice_seconds)} en vocal${m.longest_call > 0 ? `, avec un appel de ${duree(m.longest_call)}` : ''}.` : '',
    emoji?.emoji ? `Ton emoji ${r.kind === 'year' ? 'de l’année' : 'du mois'} : ${emoji.emoji}` : '',
  ])
}

function trophees(r: Recap): Ecran | null {
  const serie = r.streak.global ?? 0
  const quandDebloques = r.kind === 'year' ? `en ${r.period.slice(0, 4)}` : enMois(r.from)
  if (!r.trophies.length && serie < 2) return null
  let e: Ecran
  if (!r.trophies.length) e = ecran('trophees', 'Séries de jours', [{ texte: pluriel(serie, 'jour', 'jours') }], [{ texte: 'd’affilée, ta ' }, { texte: 'plus longue série', fort: true }, { texte: r.kind === 'year' ? ' de l’année.' : ' du mois.' }], [])
  else {
    const debloques = r.trophies.length >= 2 ? 'débloqués' : 'débloqué'
    const titre: Morceau[] = serie >= 2
      ? [{ texte: `${debloques} ${quandDebloques}, et une série de ` }, { texte: pluriel(serie, 'jour', 'jours'), fort: true }, { texte: '.' }]
      : [{ texte: `${debloques} ` }, { texte: quandDebloques, fort: true }, { texte: '.' }]
    e = ecran('trophees', 'Trophées', [{ texte: pluriel(r.trophies.length, 'trophée', 'trophées') }], titre, [])
  }
  e.trophees = r.trophies.slice(0, MEDAILLES)
  e.reste = Math.max(0, r.trophies.length - MEDAILLES)
  return e
}

export function titreRecap(r: Pick<Recap, 'kind' | 'period' | 'from'>): string {
  return r.kind === 'year' ? `Récap · année ${r.period.slice(0, 4)}` : `Récap · ${MOIS[versDate(r.from).getUTCMonth()]} ${r.period.slice(0, 4)}`
}

export function ecransDuRecap(r: Recap, nom: NomSalon): Ecran[] {
  const suite = [usage(r), entracte(r), sillon(r), messagerie(r, nom), trophees(r)].filter((e): e is Ecran => e !== null)
  const total = suite.length + 1
  const de = deMois(r.from)
  const coupe = de.startsWith('d’') ? 2 : 3
  const grand: Morceau[] = r.kind === 'year'
    ? [{ texte: 'Ton année ' }, { texte: r.period.slice(0, 4), fort: true }]
    : [{ texte: `Ton mois ${de.slice(0, coupe)}` }, { texte: de.slice(coupe), fort: true }]
  const ouverture = ecran('ouverture', r.kind === 'year' ? 'Récap de l’année' : 'Récap du mois', grand, [], [
    total >= 2 ? `${resumeAnnonce(total)}. Touche à droite pour avancer, à gauche pour revenir.` : 'Peu de choses à raconter sur cette période.',
  ])
  return [ouverture, ...suite]
}

export const NOMBRES = ['Zéro', 'Un', 'Deux', 'Trois', 'Quatre', 'Cinq', 'Six', 'Sept', 'Huit', 'Neuf', 'Dix']

export function resumeAnnonce(nb: number): string {
  const mot = NOMBRES[nb] ?? String(nb)
  return `${mot} ${nb >= 2 ? 'écrans' : 'écran'}`
}

export interface CaseRecap { periode: string; label: string; recap: RecapResume | null }
export interface Rangement { nouveau: RecapResume | null; mois: CaseRecap[]; annees: CaseRecap[] }

const moisPrecedent = (periode: string): string => {
  const [a, m] = periode.split('-').map(Number)
  return m <= 1 ? `${a - 1}-12` : `${a}-${String(m - 1).padStart(2, '0')}`
}

export function rangerRecaps(recaps: RecapResume[]): Rangement {
  const parPeriode = (genre: 'month' | 'year') => [...recaps].filter((r) => r.kind === genre).sort((a, b) => (a.period < b.period ? 1 : -1))
  const mois = parPeriode('month')
  const cases: CaseRecap[] = []
  if (mois.length) {
    const connus = new Map(mois.map((r) => [r.period, r]))
    const dernier = mois[mois.length - 1].period
    for (let p = mois[0].period, garde = 0; p >= dernier && garde < 600; p = moisPrecedent(p), garde++) cases.push({ periode: p, label: libelleRecap('month', p), recap: connus.get(p) ?? null })
  }
  const nouveau = [...recaps].sort((a, b) => (a.created_at < b.created_at ? 1 : -1)).find((r) => !r.seen) ?? null
  return {
    nouveau,
    mois: cases.filter((c) => !nouveau || c.recap?.id !== nouveau.id),
    annees: parPeriode('year').filter((r) => r.id !== nouveau?.id).map((r) => ({ periode: r.period, label: libelleRecap('year', r.period), recap: r })),
  }
}

export function prochainRecap(aujourdhui: string): string {
  const d = versDate(aujourdhui)
  return `1er ${MOIS[(d.getUTCMonth() + 1) % 12]}`
}
