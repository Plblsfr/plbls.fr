import { shortDuration, tint } from './films'

export type GenreBadge = 'movie' | 'season' | 'series'

export interface FilmBadge {
  kind: GenreBadge
  title_id: string
  season: number | null
  title: string
  season_title: string | null
  year: number | null
  poster: boolean
  season_poster: boolean
  episodes: number | null
  duration: number | null
  obtained_at: string
}

export interface BadgeRef { kind: GenreBadge; title_id: string; season: number | null; obtained_at: string }

export const cleBadge = (b: Pick<BadgeRef, 'kind' | 'title_id' | 'season'>) => (b.kind === 'season' ? `season:${b.title_id}:${b.season}` : `${b.kind}:${b.title_id}`)

export function etiquetteBadge(b: Pick<BadgeRef, 'kind' | 'season'>): string {
  if (b.kind === 'movie') return 'Film terminé'
  if (b.kind === 'season') return `Saison ${b.season} terminée`
  return 'Série terminée'
}

export const iconeBadge = (kind: GenreBadge) => (kind === 'movie' ? 'film' : 'tele')

export function dateBadge(iso: string): string {
  return new Date(iso).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' })
}

export function detailsBadge(b: FilmBadge): string {
  const episodes = b.episodes ? `${b.episodes} épisode${b.episodes > 1 ? 's' : ''}` : ''
  return [b.kind === 'movie' ? b.year : null, episodes, b.duration ? shortDuration(b.duration) : ''].filter(Boolean).join(' · ')
}

export function sousTitreBadge(b: FilmBadge): string {
  if (b.kind !== 'season' || !b.season_title) return ''
  return b.season_title.trim().toLowerCase() === `saison ${b.season}` ? '' : b.season_title
}

export function texteBadge(b: FilmBadge): string {
  return `${etiquetteBadge(b)} : ${b.title}${b.kind === 'movie' && b.year ? ` (${b.year})` : ''}, le ${dateBadge(b.obtained_at)}.`
}

export function afficheBadge(b: FilmBadge): { id: string; image: boolean } {
  if (b.kind === 'season' && b.season_poster) return { id: `${b.title_id}~${b.season}`, image: true }
  return { id: b.title_id, image: b.poster }
}

export function nomFichierBadge(b: FilmBadge): string {
  const slug = b.title.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'titre'
  return `badge-${slug}${b.kind === 'season' ? `-saison-${b.season}` : b.kind === 'series' ? '-serie' : ''}.png`
}

export function couper(texte: string, largeur: number, mesure: (s: string) => number, max = 2): string[] {
  const mots = texte.split(/\s+/).filter(Boolean)
  const lignes: string[] = []
  let courante = ''
  for (const mot of mots) {
    const essai = courante ? `${courante} ${mot}` : mot
    if (!courante || mesure(essai) <= largeur) { courante = essai; continue }
    lignes.push(courante)
    courante = mot
  }
  if (courante) lignes.push(courante)
  if (lignes.length <= max) return lignes
  const gardees = lignes.slice(0, max)
  let derniere = `${gardees[max - 1]} ${lignes.slice(max).join(' ')}`
  while (derniere.length > 1 && mesure(`${derniere}…`) > largeur) derniere = derniere.slice(0, -1).trimEnd()
  gardees[max - 1] = `${derniere}…`
  return gardees
}

export const CARTE = { largeur: 1080, hauteur: 1350 }

export function dessinerCarte(ctx: CanvasRenderingContext2D, b: FilmBadge, affiche: CanvasImageSource | null, police: string) {
  const { largeur: L, hauteur: H } = CARTE
  const t = tint(b.title_id)
  ctx.fillStyle = t.bg
  ctx.fillRect(0, 0, L, H)
  const halo = ctx.createRadialGradient(L / 2, 420, 60, L / 2, 420, 760)
  halo.addColorStop(0, `${t.ac}55`)
  halo.addColorStop(1, `${t.bg}00`)
  ctx.fillStyle = halo
  ctx.fillRect(0, 0, L, H)

  const aw = 500
  const ah = 750
  const ax = (L - aw) / 2
  const ay = 110
  ctx.save()
  ctx.shadowColor = 'rgba(0,0,0,0.45)'
  ctx.shadowBlur = 50
  ctx.shadowOffsetY = 20
  ctx.beginPath()
  ctx.roundRect(ax, ay, aw, ah, 28)
  ctx.fillStyle = t.ac
  ctx.fill()
  ctx.restore()
  ctx.save()
  ctx.beginPath()
  ctx.roundRect(ax, ay, aw, ah, 28)
  ctx.clip()
  if (affiche) {
    ctx.drawImage(affiche, ax, ay, aw, ah)
  } else {
    ctx.fillStyle = t.bg
    ctx.textAlign = 'center'
    ctx.font = `700 54px ${police}`
    couper(b.title, aw - 80, (s) => ctx.measureText(s).width, 4).forEach((l, i, tout) => ctx.fillText(l, L / 2, ay + ah / 2 - ((tout.length - 1) * 64) / 2 + i * 64))
  }
  ctx.restore()

  const mx = ax + aw - 10
  const my = ay + ah - 10
  ctx.beginPath()
  ctx.arc(mx, my, 74, 0, Math.PI * 2)
  ctx.fillStyle = t.ac
  ctx.fill()
  ctx.lineWidth = 10
  ctx.strokeStyle = t.bg
  ctx.stroke()
  ctx.beginPath()
  ctx.moveTo(mx - 30, my + 2)
  ctx.lineTo(mx - 8, my + 26)
  ctx.lineTo(mx + 34, my - 22)
  ctx.lineWidth = 14
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  ctx.strokeStyle = t.bg
  ctx.stroke()

  ctx.textAlign = 'center'
  ctx.textBaseline = 'alphabetic'
  let y = ay + ah + 120
  ctx.fillStyle = t.ac
  ctx.font = `700 38px ${police}`
  ctx.fillText(etiquetteBadge(b).toUpperCase(), L / 2, y)
  ctx.fillStyle = t.fg
  ctx.font = `800 66px ${police}`
  for (const l of couper(b.title, L - 160, (s) => ctx.measureText(s).width)) {
    y += 80
    ctx.fillText(l, L / 2, y)
  }
  const ligne = [sousTitreBadge(b), detailsBadge(b)].filter(Boolean).join(' · ')
  ctx.globalAlpha = 0.82
  ctx.font = `500 34px ${police}`
  if (ligne) { y += 58; ctx.fillText(couper(ligne, L - 160, (s) => ctx.measureText(s).width, 1)[0], L / 2, y) }
  y += 52
  ctx.fillText(`Regardé jusqu’au bout · ${dateBadge(b.obtained_at)}`, L / 2, y)
  ctx.globalAlpha = 0.6
  ctx.font = `600 28px ${police}`
  ctx.fillText('Entracte · plbls.fr', L / 2, H - 50)
  ctx.globalAlpha = 1
}
