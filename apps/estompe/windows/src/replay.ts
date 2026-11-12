import type { NoiseMode, OverlayCorner } from '../../../../web/estompe/src/lib/prefs.svelte'

export type Resolution = '720p' | '1080p' | '1440p' | '4k' | 'native'
export type Qualite = 'basse' | 'moyenne' | 'haute'
export type Codec = 'h264' | 'hevc'

export interface ReglagesReplay {
  actif: boolean; resolution: Resolution; ips: number; dureeS: number; qualite: Qualite; micro: boolean; curseur: boolean
  raccourci: string; dossier: string | null; quotaGo: number; jeuxAjoutes: string[]; jeuxExclus: string[]
}
export interface Voix { libelle: string | null; reduction: NoiseMode; gain: number }
export interface ReglagesMoteur {
  actif: boolean; resolution: Resolution; ips: number; dureeS: number; qualite: Qualite; micro: boolean; microLibelle: string | null; microReduction: NoiseMode
  microGain: number; curseur: boolean; dossier: string | null; quotaOctets: number; jeuxAjoutes: string[]; jeuxExclus: string[]
}
export interface Disponibilite { disponible: boolean; raison: string | null; encodeur: string | null }
export interface Options {
  resolutions: { id: Resolution; disponible: boolean; raison: string | null }[]; cadences: { ips: number; disponible: boolean; raison: string | null }[]
  dureeMaxS: number; raisonDuree: string | null; codec: Codec; debitKbps: number; memoireOctets: number
}
export interface Etat { etat: 'inactif' | 'arme' | 'pause' | 'indisponible'; jeu: string | null; raison: string | null; codec: Codec | null; largeur: number; hauteur: number; ips: number; microParDefaut: boolean }
export interface Clip {
  chemin: string; nom: string; jeu: string | null; date: string; dureeS: number; tailleOctets: number; vignette: string | null
  codec: Codec | null; largeur: number; hauteur: number; ips: number; pistesAudio: number
}
export interface ClipEnregistre { chemin: string; nom: string; jeu: string; dureeS: number; tailleOctets: number }
export interface Stockage { utiliseOctets: number; quotaOctets: number; depasse: boolean; plusAnciens: string[] }
export interface ErreurReplay { code: string; message: string }
export interface MessageOverlay { id: string; genre: 'ok' | 'info' | 'erreur'; texte: string }
export interface EtatOverlay { indicateur: boolean; messages: MessageOverlay[] }

export const CLE = 'estompe.replay'
export const ESPACE = 'clips'
export const PAGE_REGLAGES = 'replay'
export const ABSENT = 'instant replay absent de ce build'
export const GO = 1024 ** 3
export const DUREE_MIN_S = 15
export const DUREE_MAX_S = 300
export const QUOTA_MIN_GO = 1
export const RESOLUTIONS: readonly [Resolution, string][] = [['720p', '720p'], ['1080p', '1080p'], ['1440p', '1440p'], ['4k', '4K'], ['native', 'Native (celle du jeu)']]
export const CADENCES: readonly number[] = [30, 60, 120, 144]
export const QUALITES: readonly [Qualite, string][] = [['basse', 'Basse'], ['moyenne', 'Moyenne'], ['haute', 'Haute']]
export const CODECS: Record<Codec, string> = { h264: 'H.264', hevc: 'HEVC (H.265)' }
export const VOIX_NEUTRE: Voix = { libelle: null, reduction: 'strong', gain: 1 }
export const ETAT_INITIAL: Etat = { etat: 'inactif', jeu: null, raison: null, codec: null, largeur: 0, hauteur: 0, ips: 0, microParDefaut: false }
export const DEFAUTS: ReglagesReplay = {
  actif: false, resolution: '1080p', ips: 60, dureeS: 30, qualite: 'haute', micro: false, curseur: false,
  raccourci: 'Alt+F10', dossier: null, quotaGo: 20, jeuxAjoutes: [], jeuxExclus: [],
}

const borner = (n: unknown, min: number, max: number, defaut: number) => (typeof n === 'number' && Number.isFinite(n) ? Math.min(max, Math.max(min, Math.round(n))) : defaut)
const textes = (l: unknown) => (Array.isArray(l) ? [...new Set(l.filter((x): x is string => typeof x === 'string' && !!x.trim()).map((x) => x.trim()))].slice(0, 200) : [])

export function lireReglages(brut: string | null, version: string): ReglagesReplay {
  let p: Partial<ReglagesReplay> & { version?: string } = {}
  try { p = JSON.parse(brut ?? '{}') ?? {} } catch { p = {} }
  if (typeof p !== 'object') p = {}
  return {
    actif: p.actif === true && p.version === version,
    resolution: RESOLUTIONS.some(([id]) => id === p.resolution) ? p.resolution! : DEFAUTS.resolution,
    ips: CADENCES.includes(p.ips!) ? p.ips! : DEFAUTS.ips,
    dureeS: borner(p.dureeS, DUREE_MIN_S, DUREE_MAX_S, DEFAUTS.dureeS),
    qualite: QUALITES.some(([id]) => id === p.qualite) ? p.qualite! : DEFAUTS.qualite,
    micro: p.micro === true,
    curseur: p.curseur === true,
    raccourci: typeof p.raccourci === 'string' && p.raccourci ? p.raccourci : DEFAUTS.raccourci,
    dossier: typeof p.dossier === 'string' && p.dossier.trim() ? p.dossier : null,
    quotaGo: borner(p.quotaGo, QUOTA_MIN_GO, 100_000, DEFAUTS.quotaGo),
    jeuxAjoutes: textes(p.jeuxAjoutes),
    jeuxExclus: textes(p.jeuxExclus),
  }
}

export const ecrireReglages = (r: ReglagesReplay, version: string) => JSON.stringify({ ...r, version })

export function reglagesMoteur(r: ReglagesReplay, voix: Voix): ReglagesMoteur {
  const v = r.micro ? voix : VOIX_NEUTRE
  return {
    actif: r.actif, resolution: r.resolution, ips: r.ips, dureeS: r.dureeS, qualite: r.qualite, micro: r.micro,
    microLibelle: v.libelle, microReduction: v.reduction, microGain: Number.isFinite(v.gain) ? Math.min(3, Math.max(0, v.gain)) : 1,
    curseur: r.curseur, dossier: r.dossier, quotaOctets: r.quotaGo * GO, jeuxAjoutes: [...r.jeuxAjoutes], jeuxExclus: [...r.jeuxExclus],
  }
}

export function libelleMicro(peripheriques: Pick<MediaDeviceInfo, 'kind' | 'deviceId' | 'label'>[], micId: string): string | null {
  if (!micId || micId === 'default') return null
  return peripheriques.find((d) => d.kind === 'audioinput' && d.deviceId === micId)?.label.trim().slice(0, 256) || null
}

export const nomExecutable = (chemin: string) => chemin.split(/[\\/]/).pop()!.trim()

export function ajouterJeu(liste: string[], valeur: string): string[] {
  const v = valeur.trim()
  return !v || liste.some((x) => x.toLowerCase() === v.toLowerCase()) ? liste : [...liste, v]
}

export const raccourciLisible = (accelerateur: string) => accelerateur.split('+').map((t) => t.replace(/^(Key|Digit)/, '').replace(/^Control$/, 'Ctrl')).join(' + ')

export function dureeTampon(s: number): string {
  if (s < 60) return `${s} s`
  return s % 60 ? `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, '0')}` : `${s / 60} min`
}

export function dureeClip(s: number): string {
  const t = Math.max(0, Math.round(s))
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`
}

export function taille(octets: number): string {
  const virgule = (n: number) => n.toFixed(n < 10 ? 1 : 0).replace('.', ',').replace(/,0$/, '')
  if (octets >= GO) return `${virgule(octets / GO)} Go`
  if (octets >= 1024 ** 2) return `${Math.round(octets / 1024 ** 2)} Mo`
  return `${Math.max(octets ? 1 : 0, Math.round(octets / 1024))} Ko`
}

export const partUtilisee = (s: Pick<Stockage, 'utiliseOctets' | 'quotaOctets'>) => (s.quotaOctets > 0 ? Math.min(100, Math.round((s.utiliseOctets / s.quotaOctets) * 100)) : 0)

const dateFmt = new Intl.DateTimeFormat('fr-FR', { day: 'numeric', month: 'long', year: 'numeric', hour: '2-digit', minute: '2-digit' })
export function dateClip(iso: string): string {
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? '' : dateFmt.format(d)
}

export const detailsClip = (c: Pick<Clip, 'tailleOctets' | 'hauteur' | 'ips' | 'codec' | 'pistesAudio'>) =>
  [taille(c.tailleOctets), c.hauteur && `${c.hauteur}p`, c.ips && `${c.ips} i/s`, c.codec === 'hevc' && 'HEVC', c.pistesAudio > 1 && 'piste micro'].filter(Boolean).join(' · ')

const instant = (c: Pick<Clip, 'date'>) => Date.parse(c.date) || 0
export const SANS_JEU = 'Autres'
export const jeuDe = (c: Pick<Clip, 'jeu'>) => c.jeu?.trim() || SANS_JEU

export function trierClips<T extends Pick<Clip, 'date' | 'jeu' | 'nom'>>(clips: T[], jeu?: string): T[] {
  return clips.filter((c) => !jeu || jeuDe(c) === jeu).sort((a, b) => instant(b) - instant(a) || a.nom.localeCompare(b.nom, 'fr'))
}

export function jeuxDe(clips: Pick<Clip, 'jeu'>[]): { jeu: string; nombre: number }[] {
  const n = new Map<string, number>()
  for (const c of clips) n.set(jeuDe(c), (n.get(jeuDe(c)) ?? 0) + 1)
  return [...n].map(([jeu, nombre]) => ({ jeu, nombre })).sort((a, b) => Number(a.jeu === SANS_JEU) - Number(b.jeu === SANS_JEU) || a.jeu.localeCompare(b.jeu, 'fr', { sensitivity: 'base' }))
}

export type Envoi = 'direct' | 'h264' | 'leger' | 'refus'
const DEBIT_MIN_KBPS = 300
const AUDIO_KBPS = 128

export function decisionEnvoi(clip: Pick<Clip, 'codec' | 'tailleOctets' | 'pistesAudio' | 'dureeS'>, limite: number): Envoi {
  if (clip.tailleOctets > limite) {
    const debit = (limite * 8) / 1000 / Math.max(1, clip.dureeS) * 0.95 - AUDIO_KBPS
    return clip.dureeS > 0 && debit < DEBIT_MIN_KBPS ? 'refus' : 'leger'
  }
  return clip.codec === 'h264' && clip.pistesAudio <= 1 ? 'direct' : 'h264'
}

export function messageEtat(avant: Etat, apres: Etat): Omit<MessageOverlay, 'id'> | null {
  const change = avant.etat !== apres.etat || avant.raison !== apres.raison
  if (apres.etat === 'pause' && change) return { genre: 'info', texte: `Instant replay en pause${apres.raison ? ` : ${apres.raison}` : ''}` }
  if (apres.etat === 'indisponible' && change) return { genre: 'erreur', texte: `Instant replay indisponible${apres.raison ? ` : ${apres.raison}` : ''}` }
  if (apres.etat === 'arme' && avant.etat === 'pause') return { genre: 'info', texte: 'Instant replay repris' }
  if (apres.etat === 'arme' && apres.microParDefaut && !(avant.etat === 'arme' && avant.microParDefaut)) return { genre: 'info', texte: 'Micro choisi introuvable : le micro par défaut de Windows est enregistré' }
  return null
}

export const messageClip = (c: Pick<ClipEnregistre, 'dureeS'>): Omit<MessageOverlay, 'id'> => ({ genre: 'ok', texte: `Clip enregistré (${dureeClip(c.dureeS)})` })
export const messageErreur = (e: ErreurReplay): Omit<MessageOverlay, 'id'> => ({ genre: 'erreur', texte: e.message || `Instant replay : erreur (${e.code})` })

export const MESSAGES_MAX = 3
export const TAILLE_OVERLAY = { l: 320, indicateur: 26, ligne: 44, ecart: 6 }

export const ajouterMessage = (liste: MessageOverlay[], m: MessageOverlay, max = MESSAGES_MAX) => [...liste, m].slice(-max)

export function etatOverlay(etat: Pick<Etat, 'etat'>, actif: boolean, messages: MessageOverlay[]): EtatOverlay | null {
  const indicateur = actif && etat.etat === 'arme'
  return indicateur || messages.length ? { indicateur, messages } : null
}

export function tailleOverlay(s: EtatOverlay) {
  const { l, indicateur, ligne, ecart } = TAILLE_OVERLAY
  const lignes = s.messages.length + (s.indicateur ? 1 : 0)
  return { w: l, h: Math.max(1, (s.indicateur ? indicateur : 0) + s.messages.length * ligne + Math.max(0, lignes - 1) * ecart) }
}

export const coinOverlay = (participants: OverlayCorner): OverlayCorner => (participants === 'tr' ? 'tl' : 'tr')
