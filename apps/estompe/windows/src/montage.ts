import type { Codec } from './replay'

export const VERSION = 1
export const VOLUME_MAX = 2
export const HISTORIQUE_MAX = 200

export interface Segment { source: string; debut: number; fin: number; garde: boolean }
export interface Volume { volume: number; muet: boolean }
export interface Montage { version: typeof VERSION; segments: Segment[]; volumes: Record<string, Volume[]> }
export interface Source { nom: string; dureeS: number; ips: number; pistes: number }
export interface Piste { nom: string; chemin: string }
export interface Ouverture {
  chemin: string; nom: string; source: string; dureeS: number; largeur: number; hauteur: number; ips: number; codec: Codec | null
  apercu: string; pistes: Piste[]; projet: unknown
}
export interface EtatExport { id: number; source: string; nom: string; pct: number; etat: 'encours' | 'fini' | 'annule' | 'erreur'; message: string | null; chemin: string | null; logiciel: boolean }
export interface Historique { passe: Montage[]; present: Montage; futur: Montage[] }
export type Cote = 'debut' | 'fin'

export const sourceDe = (o: Pick<Ouverture, 'source' | 'dureeS' | 'ips' | 'pistes'>): Source => ({ nom: o.source, dureeS: o.dureeS, ips: o.ips, pistes: o.pistes.length })

export const imageDe = (t: number, ips: number) => Math.floor(t * ips + 0.25)
export const tempsImage = (image: number, ips: number) => (image + 0.5) / ips
export const images = (s: Source) => Math.max(1, Math.round(s.dureeS * s.ips))
export const duree = (s: Source) => images(s) / s.ips
const rang = (t: number, s: Source) => Math.min(images(s), Math.max(0, Math.round(t * s.ips)))
export const caler = (t: number, s: Source) => rang(Number.isFinite(t) ? t : 0, s) / s.ips
export const pas = (t: number, n: number, s: Source) => Math.min(images(s) - 1, Math.max(0, imageDe(t, s.ips) + n)) / s.ips

export function initial(s: Source): Montage {
  return { version: VERSION, segments: [{ source: s.nom, debut: 0, fin: duree(s), garde: true }], volumes: { [s.nom]: Array.from({ length: s.pistes }, () => ({ volume: 1, muet: false })) } }
}

export const bornes = (m: Montage) => ({ debut: m.segments[0].debut, fin: m.segments[m.segments.length - 1].fin })
export const gardes = (m: Montage) => m.segments.filter((s) => s.garde)
export const dureeFinale = (m: Montage) => gardes(m).reduce((t, s) => t + (s.fin - s.debut), 0)
export const segmentA = (m: Montage, t: number) => m.segments.findIndex((s) => t >= s.debut && t < s.fin)

export function rogner(m: Montage, s: Source, cote: Cote, t: number): Montage {
  const b = bornes(m)
  const image = 1 / s.ips
  if (cote === 'debut') {
    const d = caler(Math.min(t, b.fin - image), s)
    if (d === b.debut) return m
    const [premier, ...reste] = m.segments.filter((x) => x.fin > d)
    const tete = d < premier.debut && !premier.garde ? [{ ...premier, debut: d, fin: premier.debut, garde: true }, premier] : [{ ...premier, debut: d }]
    return { ...m, segments: [...tete, ...reste] }
  }
  const f = caler(Math.max(t, b.debut + image), s)
  if (f === b.fin) return m
  const garder = m.segments.filter((x) => x.debut < f)
  const dernier = garder[garder.length - 1]
  const queue = f > dernier.fin && !dernier.garde ? [dernier, { ...dernier, debut: dernier.fin, fin: f, garde: true }] : [{ ...dernier, fin: f }]
  return { ...m, segments: [...garder.slice(0, -1), ...queue] }
}

export function couper(m: Montage, s: Source, t: number): Montage {
  const c = caler(t, s)
  const i = m.segments.findIndex((x) => c > x.debut && c < x.fin)
  if (i < 0) return m
  const x = m.segments[i]
  return { ...m, segments: [...m.segments.slice(0, i), { ...x, fin: c }, { ...x, debut: c }, ...m.segments.slice(i + 1)] }
}

export function garder(m: Montage, i: number, garde: boolean): Montage {
  const x = m.segments[i]
  if (!x || x.garde === garde) return m
  return { ...m, segments: m.segments.map((y, j) => (j === i ? { ...y, garde } : y)) }
}
export const retirer = (m: Montage, i: number) => garder(m, i, false)
export const restaurer = (m: Montage, i: number) => garder(m, i, true)

export function regler(m: Montage, s: Source, piste: number, patch: Partial<Volume>): Montage {
  const actuels = m.volumes[s.nom] ?? []
  const avant = actuels[piste] ?? { volume: 1, muet: false }
  const brut = patch.volume ?? avant.volume
  const apres = { volume: Number.isFinite(brut) ? Math.min(VOLUME_MAX, Math.max(0, brut)) : avant.volume, muet: patch.muet ?? avant.muet }
  if (piste < 0 || piste >= s.pistes || (apres.volume === avant.volume && apres.muet === avant.muet)) return m
  return { ...m, volumes: { ...m.volumes, [s.nom]: Array.from({ length: s.pistes }, (_, i) => (i === piste ? apres : (actuels[i] ?? { volume: 1, muet: false }))) } }
}

export const gain = (m: Montage, s: Source, piste: number) => {
  const v = m.volumes[s.nom]?.[piste]
  return v ? (v.muet ? 0 : v.volume) : 1
}

export function versMontage(m: Montage, t: number): number {
  let total = 0
  for (const s of gardes(m)) {
    if (t <= s.debut) break
    total += Math.min(t, s.fin) - s.debut
  }
  return total
}

export function versSource(m: Montage, t: number): number {
  const g = gardes(m)
  let reste = Math.max(0, t)
  for (const s of g) {
    if (reste < s.fin - s.debut) return s.debut + reste
    reste -= s.fin - s.debut
  }
  return g.length ? g[g.length - 1].fin : bornes(m).debut
}

export function suite(m: Montage, t: number, ips: number): { aller: number } | { fin: true } | null {
  const i = imageDe(t, ips)
  const dans = (s: Segment) => i >= Math.round(s.debut * ips) && i < Math.round(s.fin * ips)
  const g = gardes(m)
  if (g.some(dans)) return null
  const prochain = g.find((s) => Math.round(s.debut * ips) > i)
  return prochain ? { aller: prochain.debut } : { fin: true }
}

export const historique = (m: Montage): Historique => ({ passe: [], present: m, futur: [] })
export const appliquer = (h: Historique, m: Montage): Historique => (m === h.present ? h : { passe: [...h.passe, h.present].slice(-HISTORIQUE_MAX), present: m, futur: [] })
export const annuler = (h: Historique): Historique => (h.passe.length ? { passe: h.passe.slice(0, -1), present: h.passe[h.passe.length - 1], futur: [h.present, ...h.futur] } : h)
export const retablir = (h: Historique): Historique => (h.futur.length ? { passe: [...h.passe, h.present], present: h.futur[0], futur: h.futur.slice(1) } : h)

export function lireProjet(brut: unknown, s: Source): Montage {
  const p = brut as Partial<Montage> | null
  if (!p || typeof p !== 'object' || p.version !== VERSION || !Array.isArray(p.segments) || !p.segments.length) return initial(s)
  const segments: Segment[] = []
  for (const x of p.segments) {
    if (!x || typeof x.debut !== 'number' || typeof x.fin !== 'number') return initial(s)
    const [debut, fin] = [caler(x.debut, s), caler(x.fin, s)]
    if (fin <= debut || (segments.length && debut !== segments[segments.length - 1].fin)) return initial(s)
    segments.push({ source: s.nom, debut, fin, garde: x.garde !== false })
  }
  const lus = Object.values(p.volumes && typeof p.volumes === 'object' ? p.volumes : {})[0]
  const volumes = Array.from({ length: s.pistes }, (_, i) => {
    const v = Array.isArray(lus) ? lus[i] : undefined
    const volume = typeof v?.volume === 'number' && Number.isFinite(v.volume) ? Math.min(VOLUME_MAX, Math.max(0, v.volume)) : 1
    return { volume, muet: v?.muet === true }
  })
  return { version: VERSION, segments, volumes: { [s.nom]: volumes } }
}

export function formaterTemps(t: number): string {
  const c = Math.max(0, Math.round(t * 100))
  return `${Math.floor(c / 6000)}:${String(Math.floor(c / 100) % 60).padStart(2, '0')},${String(c % 100).padStart(2, '0')}`
}

export function lireTemps(texte: string): number | null {
  const parties = texte.trim().replace(',', '.').split(':')
  if (!parties.length || parties.length > 3 || parties.some((p) => !/^\d+(\.\d+)?$/.test(p))) return null
  return parties.reduce((t, p) => t * 60 + Number(p), 0)
}

export function pics(canaux: ArrayLike<number>[], n: number): number[] {
  const longueur = Math.max(0, ...canaux.map((c) => c.length))
  const sortie = new Array<number>(n).fill(0)
  if (!longueur || n < 1) return sortie
  const saut = Math.max(1, Math.floor(longueur / n / 64))
  for (const c of canaux) {
    for (let i = 0; i < c.length; i += saut) {
      const b = Math.min(n - 1, Math.floor((i / longueur) * n))
      const v = Math.abs(c[i])
      if (v > sortie[b]) sortie[b] = v
    }
  }
  return sortie.map((v) => Math.min(1, v))
}

export function traceOnde(p: number[], hauteur = 100): string {
  const milieu = hauteur / 2
  const echelle = hauteur / 2 / (Math.max(...p, 0) || 1)
  return p.map((v, i) => { const h = Math.max(0.5, v * echelle); return `M${i + 0.5} ${(milieu - h).toFixed(1)}V${(milieu + h).toFixed(1)}` }).join('')
}

export type Action = 'lecture' | 'precedente' | 'suivante' | 'couper' | 'annuler' | 'retablir' | 'basculer'
export const RACCOURCIS: readonly { touches: string[]; libelle: string }[] = [
  { touches: ['Espace'], libelle: 'Lecture, pause' },
  { touches: ['←', '→'], libelle: 'Image précédente, suivante' },
  { touches: ['C'], libelle: 'Poser un point de coupe' },
  { touches: ['Suppr'], libelle: 'Retirer ou restaurer le segment' },
  { touches: ['Ctrl', 'Z'], libelle: 'Annuler' },
  { touches: ['Ctrl', 'Y'], libelle: 'Rétablir' },
]

export function actionClavier(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>, champ: boolean): Action | null {
  if (champ || e.altKey) return null
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key
  if (e.ctrlKey || e.metaKey) return k === 'z' ? (e.shiftKey ? 'retablir' : 'annuler') : k === 'y' ? 'retablir' : null
  if (k === ' ') return 'lecture'
  if (k === 'ArrowLeft') return 'precedente'
  if (k === 'ArrowRight') return 'suivante'
  if (k === 'c') return 'couper'
  if (k === 'Delete' || k === 'Backspace') return 'basculer'
  return null
}

export function nomTravail(e: Pick<EtatExport, 'nom' | 'logiciel'>): string {
  return `Export de « ${e.nom} »${e.logiciel ? ' (encodeur logiciel)' : ''}`
}
