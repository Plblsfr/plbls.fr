import type { Codec } from './replay'

export const VERSION = 2
export const VOLUME_MAX = 2
export const HISTORIQUE_MAX = 200
export const CLIPS_MAX = 20
export const DUREE_MAX_S = 1800
export const TEXTES_MAX = 50
export const TEXTE_MAX = 200
export const VITESSES = [0.25, 0.5, 1, 1.5, 2, 4] as const
export const RAPPORTS = ['libre', '16:9', '1:1', '9:16', '4:5'] as const
export const POLICES = ['Bricolage Grotesque', 'Instrument Sans', 'JetBrains Mono'] as const
export const HABILLAGES = [['aucun', 'Aucun'], ['contour', 'Contour'], ['fond', 'Fond']] as const
export const HAUTEURS = [2160, 1440, 1080, 720, 480] as const
export const CADENCES = [144, 120, 60, 30] as const
const CADRE_MIN = 0.05

export type Rapport = (typeof RAPPORTS)[number]
export type Habillage = (typeof HABILLAGES)[number][0]
export interface Segment { source: string; debut: number; fin: number; garde: boolean; vitesse: number; muet: boolean }
export interface Volume { volume: number; muet: boolean }
export interface Texte { contenu: string; police: string; taille: number; couleur: string; habillage: Habillage; x: number; y: number; debut: number; fin: number }
export interface Cadre { x: number; y: number; l: number; h: number; rapport: Rapport }
export interface Sortie { hauteur: number | null; ips: number | null }
export interface Montage { version: typeof VERSION; nom: string; segments: Segment[]; volumes: Record<string, Volume[]>; textes: Texte[]; cadre: Cadre; sortie: Sortie }
export interface Source { nom: string; dureeS: number; ips: number; pistes: number; largeur: number; hauteur: number }
export type Sources = Record<string, Source>
export interface Piste { nom: string; chemin: string }
export interface ClipOuvert {
  chemin: string; nom: string; source: string; dureeS: number; largeur: number; hauteur: number; ips: number; codec: Codec | null
  apercu: string; pistes: Piste[]
}
export interface Ouverture { id: string; projet: unknown; clips: ClipOuvert[]; manquants: string[] }
export interface Projet { id: string; nom: string; date: string; dureeS: number; clips: string[]; manquants: string[] }
export interface EtatExport { id: number; projet: string; nom: string; pct: number; etat: 'encours' | 'fini' | 'annule' | 'erreur'; message: string | null; chemin: string | null; logiciel: boolean }
export interface Historique { passe: Montage[]; present: Montage; futur: Montage[] }
export interface Lieu { i: number; t: number }
export type Cote = 'debut' | 'fin'
export type Boite = [l: number, h: number, x: number, y: number]

export const sourceDe = (o: Pick<ClipOuvert, 'source' | 'dureeS' | 'ips' | 'pistes' | 'largeur' | 'hauteur'>): Source => ({ nom: o.source, dureeS: o.dureeS, ips: o.ips, pistes: o.pistes.length, largeur: o.largeur, hauteur: o.hauteur })
export const sourcesDe = (clips: ClipOuvert[]): Sources => Object.fromEntries(clips.map((c) => [c.source, sourceDe(c)]))

export const imageDe = (t: number, ips: number) => Math.floor(t * ips + 0.25)
export const tempsImage = (image: number, ips: number) => (image + 0.5) / ips
export const images = (s: Source) => Math.max(1, Math.round(s.dureeS * s.ips))
export const duree = (s: Source) => images(s) / s.ips
const rang = (t: number, s: Source) => Math.min(images(s), Math.max(0, Math.round(t * s.ips)))
export const caler = (t: number, s: Source) => rang(Number.isFinite(t) ? t : 0, s) / s.ips
const borner = (x: unknown, min: number, max: number, defaut: number) => (typeof x === 'number' && Number.isFinite(x) ? Math.min(max, Math.max(min, x)) : defaut)

export const CADRE_PLEIN: Cadre = { x: 0, y: 0, l: 1, h: 1, rapport: 'libre' }
const entier = (s: Source): Segment => ({ source: s.nom, debut: 0, fin: duree(s), garde: true, vitesse: 1, muet: false })
const volumesNeufs = (s: Source): Volume[] => Array.from({ length: s.pistes }, () => ({ volume: 1, muet: false }))

export function initial(s: Source, nom = s.nom.replace(/\.mp4$/i, '')): Montage {
  return { version: VERSION, nom, segments: [entier(s)], volumes: { [s.nom]: volumesNeufs(s) }, textes: [], cadre: CADRE_PLEIN, sortie: { hauteur: null, ips: null } }
}

export function bloc(m: Montage, i: number): [number, number] {
  const colle = (j: number) => m.segments[j].source === m.segments[j + 1].source && m.segments[j].fin === m.segments[j + 1].debut
  let [a, b] = [i, i + 1]
  while (a > 0 && colle(a - 1)) a--
  while (b < m.segments.length && colle(b - 1)) b++
  return [a, b]
}
export const bornes = (m: Montage, i = 0) => { const [a, b] = bloc(m, i); return { debut: m.segments[a].debut, fin: m.segments[b - 1].fin } }
export const gardes = (m: Montage) => m.segments.filter((s) => s.garde)
export const dureeFinale = (m: Montage) => gardes(m).reduce((t, s) => t + (s.fin - s.debut) / s.vitesse, 0)
export const clipsDe = (m: Montage) => [...new Set(m.segments.map((s) => s.source))]

export function rogner(m: Montage, s: Source, cote: Cote, t: number, i = 0): Montage {
  const [a, z] = bloc(m, i)
  const morceau = m.segments.slice(a, z)
  const b = { debut: morceau[0].debut, fin: morceau[morceau.length - 1].fin }
  const image = 1 / s.ips
  const remplacer = (segments: Segment[]) => ({ ...m, segments: [...m.segments.slice(0, a), ...segments, ...m.segments.slice(z)] })
  if (cote === 'debut') {
    const d = caler(Math.min(t, b.fin - image), s)
    if (d === b.debut) return m
    const [premier, ...reste] = morceau.filter((x) => x.fin > d)
    const tete = d < premier.debut && !premier.garde ? [{ ...premier, debut: d, fin: premier.debut, garde: true }, premier] : [{ ...premier, debut: d }]
    return remplacer([...tete, ...reste])
  }
  const f = caler(Math.max(t, b.debut + image), s)
  if (f === b.fin) return m
  const garder = morceau.filter((x) => x.debut < f)
  const dernier = garder[garder.length - 1]
  const queue = f > dernier.fin && !dernier.garde ? [dernier, { ...dernier, debut: dernier.fin, fin: f, garde: true }] : [{ ...dernier, fin: f }]
  return remplacer([...garder.slice(0, -1), ...queue])
}

export function couper(m: Montage, s: Source, t: number, i?: number): Montage {
  const c = caler(t, s)
  const dans = (x: Segment | undefined) => !!x && x.source === s.nom && c > x.debut && c < x.fin
  const j = i ?? m.segments.findIndex(dans)
  const x = m.segments[j]
  if (!dans(x)) return m
  return { ...m, segments: [...m.segments.slice(0, j), { ...x, fin: c }, { ...x, debut: c }, ...m.segments.slice(j + 1)] }
}

export function reglerSegment(m: Montage, i: number, patch: Partial<Pick<Segment, 'garde' | 'vitesse' | 'muet'>>): Montage {
  const x = m.segments[i]
  const apres = x && { ...x, ...patch }
  if (!x || !(VITESSES as readonly number[]).includes(apres.vitesse) || (apres.garde === x.garde && apres.vitesse === x.vitesse && apres.muet === x.muet)) return m
  return { ...m, segments: m.segments.map((y, j) => (j === i ? apres : y)) }
}
export const garder = (m: Montage, i: number, garde: boolean) => reglerSegment(m, i, { garde })
export const retirer = (m: Montage, i: number) => garder(m, i, false)
export const restaurer = (m: Montage, i: number) => garder(m, i, true)

export function deplacer(m: Montage, de: number, vers: number): Montage {
  const n = m.segments.length
  if (de === vers || de < 0 || vers < 0 || de >= n || vers >= n) return m
  const segments = [...m.segments]
  segments.splice(vers, 0, ...segments.splice(de, 1))
  return { ...m, segments }
}

export function deplacerClip(m: Montage, nom: string, vers: number): Montage {
  const ordre = clipsDe(m)
  const de = ordre.indexOf(nom)
  if (de < 0 || vers < 0 || vers >= ordre.length) return m
  ordre.splice(vers, 0, ...ordre.splice(de, 1))
  const segments = ordre.flatMap((c) => m.segments.filter((s) => s.source === c))
  return segments.every((s, i) => s === m.segments[i]) ? m : { ...m, segments }
}

export function ajouterClip(m: Montage, s: Source): Montage {
  if (clipsDe(m).includes(s.nom)) return m
  return { ...m, segments: [...m.segments, entier(s)], volumes: { ...m.volumes, [s.nom]: volumesNeufs(s) } }
}

export function retirerClip(m: Montage, nom: string): Montage {
  const segments = m.segments.filter((s) => s.source !== nom)
  if (!segments.length || segments.length === m.segments.length) return m
  const { [nom]: _, ...volumes } = m.volumes
  return { ...m, segments, volumes }
}

export function refus(avant: Montage, apres: Montage): string | null {
  if (clipsDe(apres).length > Math.max(CLIPS_MAX, clipsDe(avant).length)) return `Un montage réunit ${CLIPS_MAX} clips au plus : ce clip n’a pas été ajouté.`
  if (dureeFinale(apres) > Math.max(DUREE_MAX_S, dureeFinale(avant))) return `Un montage dure ${DUREE_MAX_S / 60} minutes au plus : la modification n’a pas été appliquée.`
  return null
}

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

export function axe(m: Montage): { debuts: number[]; total: number } {
  let total = 0
  const debuts = m.segments.map((s) => { const d = total; total += s.fin - s.debut; return d })
  return { debuts, total }
}

export function lieu(m: Montage, u: number): Lieu {
  const { debuts } = axe(m)
  let i = debuts.findIndex((d, j) => u < d + m.segments[j].fin - m.segments[j].debut)
  if (i < 0) i = m.segments.length - 1
  const s = m.segments[i]
  return { i, t: Math.min(s.fin, Math.max(s.debut, s.debut + u - debuts[i])) }
}

export const position = (m: Montage, l: Lieu) => axe(m).debuts[l.i] + l.t - m.segments[l.i].debut

export function versMontage(m: Montage, u: number): number {
  const { debuts } = axe(m)
  return m.segments.reduce((t, s, i) => (s.garde ? t + Math.min(s.fin - s.debut, Math.max(0, u - debuts[i])) / s.vitesse : t), 0)
}

export function versAxe(m: Montage, t: number): number {
  const { debuts, total } = axe(m)
  let reste = Math.max(0, t)
  let dernier = 0
  for (const [i, s] of m.segments.entries()) {
    if (!s.garde) continue
    const d = (s.fin - s.debut) / s.vitesse
    if (reste < d) return debuts[i] + reste * s.vitesse
    reste -= d
    dernier = debuts[i] + s.fin - s.debut
  }
  return gardes(m).length ? dernier : Math.min(total, 0)
}

export function suite(m: Montage, l: Lieu, sources: Sources): { aller: Lieu } | { fin: true } | null {
  const s = m.segments[l.i]
  const ips = sources[s.source]?.ips ?? 0
  if (s.garde && ips && imageDe(l.t, ips) >= Math.round(s.debut * ips) && imageDe(l.t, ips) < Math.round(s.fin * ips)) return null
  const avant = s.garde && ips && imageDe(l.t, ips) < Math.round(s.debut * ips)
  const j = avant ? l.i : m.segments.findIndex((x, k) => k > l.i && x.garde)
  return j < 0 ? { fin: true } : { aller: { i: j, t: m.segments[j].debut } }
}

export function pas(m: Montage, l: Lieu, n: number, sources: Sources): Lieu {
  const s = m.segments[l.i]
  const ips = sources[s.source].ips
  const [premiere, derniere] = [Math.round(s.debut * ips), Math.round(s.fin * ips) - 1]
  const image = Math.min(derniere, Math.max(premiere, imageDe(l.t, ips))) + n
  const voisin = m.segments[l.i + Math.sign(n)]
  if (image >= premiere && image <= derniere) return { i: l.i, t: image / ips }
  if (!voisin) return { i: l.i, t: (n < 0 ? premiere : derniere) / ips }
  const cadence = sources[voisin.source].ips
  return { i: l.i + Math.sign(n), t: n < 0 ? (Math.round(voisin.fin * cadence) - 1) / cadence : voisin.debut }
}

export const saccade = (s: Pick<Segment, 'vitesse'>, source: Pick<Source, 'ips'>, ipsSortie: number) => source.ips * s.vitesse < ipsSortie

const pair = (x: number) => Math.max(2, Math.round(x / 2) * 2)
export const toile = (largeur: number, hauteur: number): [number, number] => [Math.max(2, largeur - (largeur % 2)), Math.max(2, hauteur - (hauteur % 2))]

export function inscrire(largeur: number, hauteur: number, t: [number, number]): Boite {
  const echelle = Math.min(t[0] / Math.max(1, largeur), t[1] / Math.max(1, hauteur))
  const [l, h] = [Math.min(t[0], pair(largeur * echelle)), Math.min(t[1], pair(hauteur * echelle))]
  return [l, h, Math.floor((t[0] - l) / 4) * 2, Math.floor((t[1] - h) / 4) * 2]
}

export function cadrePixels(c: Cadre, t: [number, number]): Boite {
  const [l, h] = [Math.min(t[0], pair(c.l * t[0])), Math.min(t[1], pair(c.h * t[1]))]
  const coin = (f: number, total: number, taille: number) => Math.min(total - taille, Math.max(0, Math.round((f * total) / 2)) * 2)
  return [l, h, coin(c.x, t[0], l), coin(c.y, t[1], h)]
}

const proportions = (r: Rapport) => (r === 'libre' ? null : (r.split(':').map(Number) as [number, number]))

export function dimensionsSortie(cadre: [number, number], rapport: Rapport, hauteur: number | null): [number, number] {
  const [l, h] = cadre
  const voulue = Math.min(h, hauteur ?? h)
  const p = proportions(rapport)
  if (p) {
    const k = Math.max(2, Math.floor(Math.min(l / p[0], h / p[1], voulue / p[1]) / 2) * 2)
    return [p[0] * k, p[1] * k]
  }
  const haut = Math.min(h, pair(voulue))
  return [haut === h ? l : Math.min(l, pair((l * haut) / h)), haut]
}

export function bornerCadre(c: Cadre, t: [number, number]): Cadre {
  const p = proportions(c.rapport)
  const r = p && p[0] / p[1] / (t[0] / t[1])
  let [l, h] = [borner(c.l, CADRE_MIN, 1, 1), borner(c.h, CADRE_MIN, 1, 1)]
  if (r) { l = Math.min(l, 1, r); h = l / r }
  return { x: borner(c.x, 0, 1 - l, 0), y: borner(c.y, 0, 1 - h, 0), l, h, rapport: c.rapport }
}

export function cadreAu(rapport: Rapport, c: Cadre, t: [number, number]): Cadre {
  const plein = bornerCadre({ ...CADRE_PLEIN, rapport }, t)
  return rapport === 'libre' ? { ...c, rapport } : bornerCadre({ ...plein, x: c.x + c.l / 2 - plein.l / 2, y: c.y + c.h / 2 - plein.h / 2 }, t)
}

export function recadrer(m: Montage, c: Cadre, t: [number, number]): Montage {
  const apres = bornerCadre(c, t)
  const a = m.cadre
  return apres.x === a.x && apres.y === a.y && apres.l === a.l && apres.h === a.h && apres.rapport === a.rapport ? m : { ...m, cadre: apres }
}

export interface Format { toile: [number, number]; cadre: Boite; largeur: number; hauteur: number; ips: number }
export function formatDe(m: Montage, sources: Sources): Format {
  const premier = sources[m.segments[0].source]
  const t = toile(premier.largeur, premier.hauteur)
  const cadre = cadrePixels(m.cadre, t)
  const [largeur, hauteur] = dimensionsSortie([cadre[0], cadre[1]], m.cadre.rapport, m.sortie.hauteur)
  const max = Math.max(...clipsDe(m).map((c) => sources[c].ips))
  return { toile: t, cadre, largeur, hauteur, ips: Math.min(max, m.sortie.ips ?? premier.ips) }
}
export const hauteursPossibles = (f: Format) => HAUTEURS.filter((h) => h < f.cadre[1])
export const cadencesPossibles = (m: Montage, sources: Sources) => { const max = Math.max(...clipsDe(m).map((c) => sources[c].ips)); return [...new Set([sources[m.segments[0].source].ips, ...CADENCES.filter((c) => c <= max)])].sort((a, b) => b - a) }

export function reglerSortie(m: Montage, patch: Partial<Sortie>): Montage {
  const apres = { ...m.sortie, ...patch }
  return apres.hauteur === m.sortie.hauteur && apres.ips === m.sortie.ips ? m : { ...m, sortie: apres }
}

const nettoyer = (contenu: string) => [...contenu.replace(/\r/g, '').replace(/[^\P{Cc}\n]/gu, '')].slice(0, TEXTE_MAX).join('')

export function texteNeuf(m: Montage, debut: number): Texte {
  const d = Math.max(0, Math.min(debut, dureeFinale(m) - 0.5))
  return { contenu: 'Texte', police: POLICES[0], taille: 0.08, couleur: '#ffffff', habillage: 'contour', x: 0.5, y: 0.82, debut: d, fin: Math.max(d + 0.1, Math.min(dureeFinale(m), d + 3)) }
}

export function bornerTexte(t: Texte, finale = Number.POSITIVE_INFINITY): Texte {
  const debut = borner(t.debut, 0, Math.max(0, finale - 0.1), 0)
  return {
    contenu: nettoyer(typeof t.contenu === 'string' ? t.contenu : ''), police: (POLICES as readonly string[]).includes(t.police) ? t.police : POLICES[0], taille: borner(t.taille, 0.02, 0.4, 0.08),
    couleur: /^#[0-9a-f]{6}$/i.test(t.couleur) ? t.couleur.toLowerCase() : '#ffffff', habillage: HABILLAGES.some(([h]) => h === t.habillage) ? t.habillage : 'contour',
    x: borner(t.x, 0, 1, 0.5), y: borner(t.y, 0, 1, 0.5), debut, fin: Math.max(debut + 0.1, borner(t.fin, 0, Math.max(finale, debut + 0.1), debut + 3)),
  }
}

export function ajouterTexte(m: Montage, debut: number): Montage {
  return m.textes.length >= TEXTES_MAX ? m : { ...m, textes: [...m.textes, texteNeuf(m, debut)] }
}

export function reglerTexte(m: Montage, i: number, patch: Partial<Texte>): Montage {
  const avant = m.textes[i]
  if (!avant) return m
  const apres = bornerTexte({ ...avant, ...patch }, dureeFinale(m))
  if (!apres.contenu.trim() || (Object.keys(apres) as (keyof Texte)[]).every((k) => apres[k] === avant[k])) return m
  return { ...m, textes: m.textes.map((t, j) => (j === i ? apres : t)) }
}

export const retirerTexte = (m: Montage, i: number): Montage => (m.textes[i] ? { ...m, textes: m.textes.filter((_, j) => j !== i) } : m)
export const textesA = (m: Montage, t: number) => m.textes.flatMap((x, i) => (t >= x.debut && t < x.fin ? [i] : []))

export const policeCss = (t: Pick<Texte, 'police'>, px: number) => `700 ${px}px '${t.police} Variable', '${t.police}', system-ui, sans-serif`
const contraste = (couleur: string) => { const n = Number.parseInt(couleur.slice(1), 16); return (n >> 16) * 0.299 + ((n >> 8) & 255) * 0.587 + (n & 255) * 0.114 > 140 ? '#000000' : '#ffffff' }

export interface Pinceau {
  font: string; textAlign: string; textBaseline: string; lineJoin: string; lineWidth: number; fillStyle: unknown; strokeStyle: unknown; globalAlpha: number
  clearRect(x: number, y: number, l: number, h: number): void; fillRect(x: number, y: number, l: number, h: number): void
  measureText(texte: string): { width: number }; fillText(texte: string, x: number, y: number): void; strokeText(texte: string, x: number, y: number): void
}

export const corps = (t: Pick<Texte, 'taille'>, largeur: number, hauteur: number) => Math.max(1, Math.round(t.taille * Math.min(largeur, hauteur)))

export function dessinerTexte(c: Pinceau, t: Texte, largeur: number, hauteur: number): { x: number; y: number; l: number; h: number } {
  const px = corps(t, largeur, hauteur)
  const lignes = t.contenu.split('\n')
  const [interligne, marge] = [px * 1.25, t.habillage === 'aucun' ? 0 : px * (t.habillage === 'fond' ? 0.35 : 0.1)]
  c.clearRect(0, 0, largeur, hauteur)
  c.font = policeCss(t, px)
  c.textAlign = 'center'
  c.textBaseline = 'middle'
  const [l, h] = [Math.max(...lignes.map((x) => c.measureText(x).width)) + marge * 2, lignes.length * interligne + marge * 2]
  const centrer = (f: number, total: number, taille: number) => (taille >= total ? total / 2 : Math.min(total - taille / 2, Math.max(taille / 2, f * total)))
  const [cx, cy] = [centrer(t.x, largeur, l), centrer(t.y, hauteur, h)]
  if (t.habillage === 'fond') {
    c.globalAlpha = 0.72
    c.fillStyle = contraste(t.couleur)
    c.fillRect(cx - l / 2, cy - h / 2, l, h)
    c.globalAlpha = 1
  }
  c.fillStyle = t.couleur
  c.strokeStyle = contraste(t.couleur)
  c.lineJoin = 'round'
  c.lineWidth = px * 0.16
  lignes.forEach((ligne, i) => {
    const y = cy + (i - (lignes.length - 1) / 2) * interligne
    if (t.habillage === 'contour') c.strokeText(ligne, cx, y)
    c.fillText(ligne, cx, y)
  })
  return { x: (cx - l / 2) / largeur, y: (cy - h / 2) / hauteur, l: l / largeur, h: h / hauteur }
}

export const historique = (m: Montage): Historique => ({ passe: [], present: m, futur: [] })
export const appliquer = (h: Historique, m: Montage): Historique => (m === h.present ? h : { passe: [...h.passe, h.present].slice(-HISTORIQUE_MAX), present: m, futur: [] })
export const annuler = (h: Historique): Historique => (h.passe.length ? { passe: h.passe.slice(0, -1), present: h.passe[h.passe.length - 1], futur: [h.present, ...h.futur] } : h)
export const retablir = (h: Historique): Historique => (h.futur.length ? { passe: [...h.passe, h.present], present: h.futur[0], futur: h.futur.slice(1) } : h)

export function lireProjet(brut: unknown, clips: Source[], nom: string): Montage {
  const sources: Sources = Object.fromEntries(clips.map((c) => [c.nom, c]))
  const neuf = initial(clips[0], nom)
  const p = brut as Partial<Montage> | null
  if (!p || typeof p !== 'object' || p.version !== VERSION || !Array.isArray(p.segments)) return neuf
  const segments = p.segments.flatMap((x): Segment[] => {
    const s = x && sources[x.source]
    if (!s || typeof x.debut !== 'number' || typeof x.fin !== 'number') return []
    const [debut, fin] = [caler(x.debut, s), caler(x.fin, s)]
    return fin > debut ? [{ source: s.nom, debut, fin, garde: x.garde !== false, vitesse: (VITESSES as readonly number[]).includes(x.vitesse) ? x.vitesse : 1, muet: x.muet === true }] : []
  })
  if (!segments.length) return neuf
  const lus = p.volumes && typeof p.volumes === 'object' ? p.volumes : {}
  const volumes = Object.fromEntries([...new Set(segments.map((s) => s.source))].map((c) => [c, Array.from({ length: sources[c].pistes }, (_, i) => {
    const v = Array.isArray(lus[c]) ? lus[c][i] : undefined
    return { volume: borner(v?.volume, 0, VOLUME_MAX, 1), muet: v?.muet === true }
  })]))
  const m: Montage = { ...neuf, nom: typeof p.nom === 'string' && p.nom.trim() ? p.nom : nom, segments, volumes, sortie: { hauteur: p.sortie?.hauteur ? borner(p.sortie.hauteur, 2, 4320, 1080) : null, ips: p.sortie?.ips ? borner(p.sortie.ips, 1, 240, 60) : null } }
  const premier = sources[segments[0].source]
  const rapport = RAPPORTS.includes(p.cadre?.rapport as Rapport) ? p.cadre!.rapport : 'libre'
  const textes = (Array.isArray(p.textes) ? p.textes : []).filter((t) => t && typeof t === 'object').slice(0, TEXTES_MAX).map((t) => bornerTexte(t)).filter((t) => t.contenu.trim())
  return { ...m, textes, cadre: bornerCadre({ ...CADRE_PLEIN, ...(p.cadre && typeof p.cadre === 'object' ? p.cadre : {}), rapport }, toile(premier.largeur, premier.hauteur)) }
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
