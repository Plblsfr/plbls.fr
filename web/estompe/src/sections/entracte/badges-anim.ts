import { couper, dateBadge, etiquetteBadge, type FilmBadge } from './badges'
import { shortDuration, tint } from './films'

export const STYLES = ['ticket', 'polaroid', 'pellicule', 'generique', 'amorce'] as const
export type StyleBadge = (typeof STYLES)[number]
export const DUREES: Record<StyleBadge, number> = { ticket: 5.5, polaroid: 5.5, pellicule: 5.5, generique: 6.5, amorce: 5.5 }
export const CARTE = { largeur: 1080, hauteur: 1350 }

export interface Palette { bg: string; fg: string; ac: string; ac2: string }
export interface Polices { titre: string; texte: string; serif: string; mono: string }
export type Affiche = { width: number; height: number } & CanvasImageSource

const W = CARTE.largeur
const H = CARTE.hauteur

export function hachage(s: string): number {
  let h = 0
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0
  return h
}

export const styleDe = (titre: string): StyleBadge => STYLES[hachage(`style:${titre}`) % STYLES.length]

const hex = (n: number) => Math.round(Math.min(255, Math.max(0, n))).toString(16).padStart(2, '0')
const couleur = ([r, g, b]: number[]) => `#${hex(r)}${hex(g)}${hex(b)}`

function teinte([r, g, b]: number[]): [number, number, number] {
  const max = Math.max(r, g, b) / 255
  const min = Math.min(r, g, b) / 255
  const l = (max + min) / 2
  const d = max - min
  if (!d) return [0, 0, l]
  const s = d / (1 - Math.abs(2 * l - 1))
  const rr = r / 255
  const gg = g / 255
  const bb = b / 255
  const h = max === rr ? ((gg - bb) / d) % 6 : max === gg ? (bb - rr) / d + 2 : (rr - gg) / d + 4
  return [(h * 60 + 360) % 360, s, l]
}

function rgb(h: number, s: number, l: number): number[] {
  const c = (1 - Math.abs(2 * l - 1)) * s
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1))
  const m = l - c / 2
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x]
  return [(r + m) * 255, (g + m) * 255, (b + m) * 255]
}

export function paletteDe(pixels: Uint8ClampedArray | null, titre: string): Palette {
  const t = tint(titre)
  if (!pixels || pixels.length < 4) return { bg: t.bg, fg: t.fg, ac: t.ac, ac2: t.ac }
  let somme = [0, 0, 0]
  let n = 0
  let vive: number[] | null = null
  let score = -1
  for (let i = 0; i + 3 < pixels.length; i += 4) {
    if (pixels[i + 3] < 128) continue
    const p = [pixels[i], pixels[i + 1], pixels[i + 2]]
    somme = somme.map((v, k) => v + p[k])
    n++
    const [, s, l] = teinte(p)
    const sc = s * (1 - Math.abs(l - 0.5) * 2)
    if (sc > score) { score = sc; vive = p }
  }
  if (!n || !vive) return { bg: t.bg, fg: t.fg, ac: t.ac, ac2: t.ac }
  const [hm, sm] = teinte(somme.map((v) => v / n))
  const [hv, sv] = teinte(vive)
  const ac = score > 0.15 ? rgb(hv, Math.max(0.55, sv), 0.55) : rgb(hm, 0.5, 0.6)
  return {
    bg: couleur(rgb(hm, Math.min(0.6, sm), 0.1)),
    fg: '#f7f1e6',
    ac: couleur(ac),
    ac2: couleur(rgb((hv + 25) % 360, Math.max(0.5, sv), 0.65)),
  }
}

const cl = (x: number, a = 0, b = 1) => Math.min(b, Math.max(a, x))
const prog = (t: number, a: number, b: number) => cl((t - a) / (b - a))
const sortie = (x: number) => 1 - (1 - x) ** 3
const douce = (x: number) => (x < 0.5 ? 4 * x * x * x : 1 - (-2 * x + 2) ** 3 / 2)
const rebond = (x: number) => 1 + 2.70158 * (x - 1) ** 3 + 1.70158 * (x - 1) ** 2
const ressort = (x: number) => (x === 0 || x === 1 ? x : 2 ** (-10 * x) * Math.sin((x * 10 - 0.75) * ((2 * Math.PI) / 3)) + 1)
const tape = (s: string, x: number) => s.slice(0, Math.round(s.length * x))

const duree = (b: FilmBadge) => (b.duration ? shortDuration(b.duration) : '')
export function detailsAnim(b: FilmBadge): string {
  const episodes = b.episodes ? `${b.episodes} épisode${b.episodes > 1 ? 's' : ''}` : ''
  return [b.kind === 'movie' ? b.year : null, episodes, duree(b)].filter(Boolean).join(' · ')
}
const dateCourte = (b: FilmBadge) => new Date(b.obtained_at).toLocaleDateString('fr-FR', { day: '2-digit', month: '2-digit', year: 'numeric' })

type Ctx = CanvasRenderingContext2D

function lignes(ctx: Ctx, texte: string, x: number, y: number, largeur: number, hauteur: number, max = 2) {
  const ls = couper(texte, largeur, (s) => ctx.measureText(s).width, max)
  ls.forEach((l, i) => ctx.fillText(l, x, y + i * hauteur))
  return y + Math.max(0, ls.length - 1) * hauteur
}

function rr(ctx: Ctx, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath()
  ctx.roundRect(x, y, w, h, r)
}

interface Scene { ctx: Ctx; b: FilmBadge; img: Affiche | null; p: Palette; f: Polices; t: number }

function couvrir({ ctx, b, img, p, f }: Scene, x: number, y: number, w: number, h: number) {
  if (!img) {
    ctx.fillStyle = p.ac
    ctx.fillRect(x, y, w, h)
    ctx.fillStyle = p.bg
    ctx.textAlign = 'center'
    const taille = Math.max(24, Math.round(w / 9))
    ctx.font = `800 ${taille}px ${f.titre}`
    const ls = couper(b.title, w - 40, (s) => ctx.measureText(s).width, 4)
    ls.forEach((l, i) => ctx.fillText(l, x + w / 2, y + h / 2 - ((ls.length - 1) * taille * 1.15) / 2 + i * taille * 1.15))
    return
  }
  const s = Math.max(w / img.width, h / img.height)
  const sw = w / s
  const sh = h / s
  ctx.drawImage(img, (img.width - sw) / 2, (img.height - sh) / 2, sw, sh, x, y, w, h)
}

function grain(ctx: Ctx, alpha: number, graine: number) {
  let s = graine + 1
  const r = () => (s = (s * 16807) % 2147483647) / 2147483647
  ctx.save()
  ctx.globalAlpha = alpha
  for (let i = 0; i < 6000; i++) {
    ctx.fillStyle = r() > 0.5 ? '#fff' : '#000'
    ctx.fillRect(r() * W, r() * H, 2, 2)
  }
  ctx.restore()
}

function tampon(b: FilmBadge) {
  return { haut: b.kind === 'movie' ? 'FILM' : b.kind === 'season' ? `SAISON ${b.season}` : 'SÉRIE', bas: b.kind === 'movie' ? 'TERMINÉ' : 'TERMINÉE' }
}

function ticket(sc: Scene) {
  const { ctx, p, t } = sc
  ctx.fillStyle = p.ac
  ctx.fillRect(0, 0, W, H)
  ctx.save()
  ctx.globalAlpha = 0.18
  ctx.fillStyle = '#000'
  const dec = (t * 60) % 48
  for (let i = -H - 48; i < W; i += 48) {
    ctx.beginPath()
    ctx.moveTo(i + dec, 0)
    ctx.lineTo(i + dec + H, H)
    ctx.lineTo(i + dec + H + 22, H)
    ctx.lineTo(i + dec + 22, 0)
    ctx.fill()
  }
  ctx.restore()
  const e = rebond(prog(t, 0.1, 0.9))
  const tw = 820
  const th = 1080
  const x = -tw / 2
  const y = -th / 2
  const coupe = y + 820
  const dechire = douce(prog(t, 3.2, 4.0))
  ctx.save()
  ctx.translate(W / 2, H / 2 + (1 - e) * 1100)
  ctx.rotate(-0.06 - (1 - e) * 0.4)
  ctx.save()
  ctx.beginPath()
  ctx.rect(x - 60, y - 60, tw + 120, coupe - y + 60)
  ctx.clip()
  billet(sc, x, y, tw, th, coupe)
  ctx.restore()
  ctx.save()
  ctx.translate(dechire * 40, dechire * 150)
  ctx.rotate(dechire * 0.12)
  ctx.beginPath()
  ctx.rect(x - 60, coupe, tw + 120, th)
  ctx.clip()
  billet(sc, x, y, tw, th, coupe)
  ctx.restore()
  ctx.restore()
}

function billet(sc: Scene, x: number, y: number, tw: number, th: number, coupe: number) {
  const { ctx, b, p, f, t } = sc
  ctx.save()
  ctx.shadowColor = 'rgba(0,0,0,.4)'
  ctx.shadowBlur = 40
  ctx.shadowOffsetY = 18
  ctx.fillStyle = '#f7efdc'
  rr(ctx, x, y, tw, th, 18)
  ctx.fill()
  ctx.restore()
  ctx.fillStyle = p.ac
  for (const cx of [x, x + tw]) {
    ctx.beginPath()
    ctx.arc(cx, coupe, 34, 0, Math.PI * 2)
    ctx.fill()
  }
  ctx.strokeStyle = '#b9ab8a'
  ctx.setLineDash([14, 12])
  ctx.lineWidth = 4
  ctx.beginPath()
  ctx.moveTo(x + 50, coupe)
  ctx.lineTo(x + tw - 50, coupe)
  ctx.stroke()
  ctx.setLineDash([])
  const ia = sortie(prog(t, 0.7, 1.3))
  ctx.save()
  rr(ctx, x + 50, y + 50, 300, 450, 10)
  ctx.clip()
  ctx.fillStyle = '#222'
  ctx.fillRect(x + 50, y + 50, 300, 450)
  ctx.globalAlpha = ia
  couvrir(sc, x + 50, y + 50 + (1 - ia) * 30, 300, 450)
  ctx.restore()
  ctx.fillStyle = '#2b2418'
  ctx.textAlign = 'left'
  ctx.font = `700 26px ${f.mono}`
  ctx.fillText(tape('ENTRACTE · SÉANCE', prog(t, 0.8, 1.2)), x + 390, y + 90)
  ctx.font = `800 64px ${f.titre}`
  const yy = lignes(ctx, tape(b.title, prog(t, 1.0, 1.7)), x + 390, y + 170, 380, 70, 3)
  ctx.font = `500 28px ${f.mono}`
  ctx.fillStyle = '#6b5d43'
  ctx.fillText(tape(detailsAnim(b), prog(t, 1.6, 2.0)), x + 390, Math.max(yy, y + 240) + 60)
  const pop = ressort(prog(t, 2.0, 2.9))
  if (pop > 0) {
    ctx.save()
    ctx.translate(x + 580, y + 455)
    ctx.scale(pop, pop)
    ctx.rotate((1 - pop) * 0.3)
    ctx.fillStyle = p.ac
    rr(ctx, -190, -45, 380, 90, 10)
    ctx.fill()
    ctx.fillStyle = '#fff'
    ctx.font = `800 34px ${f.titre}`
    ctx.textAlign = 'center'
    ctx.fillText(couper(etiquetteBadge(b).toUpperCase(), 350, (s) => ctx.measureText(s).width, 1)[0], 0, 12)
    ctx.restore()
  }
  ctx.textAlign = 'left'
  const cols: [string, string][] = [['DATE', dateCourte(b)], ['RANG', 'A'], ['PLACE', String((hachage(b.title_id) % 40) + 1).padStart(2, '0')]]
  cols.forEach(([k, v], i) => {
    const a = prog(t, 1.9 + i * 0.12, 2.2 + i * 0.12)
    ctx.globalAlpha = a
    const cx = x + 50 + i * 250
    ctx.fillStyle = '#8a7a58'
    ctx.font = `600 26px ${f.mono}`
    ctx.fillText(k, cx, y + 590)
    ctx.fillStyle = '#2b2418'
    ctx.font = `700 40px ${f.mono}`
    ctx.fillText(v, cx, y + 640 + (1 - a) * 20)
  })
  ctx.globalAlpha = 1
  ctx.fillStyle = '#2b2418'
  const barres = Math.round(46 * prog(t, 2.2, 2.6))
  for (let i = 0; i < barres; i++) ctx.fillRect(x + 50 + i * 16, y + 700, ((hachage(b.title_id + i) % 3) + 2) * 2, 70)
  const scan = prog(t, 2.6, 3.1)
  if (scan > 0 && scan < 1) {
    ctx.save()
    ctx.shadowColor = '#ff2a2a'
    ctx.shadowBlur = 30
    ctx.fillStyle = '#ff3b3b'
    ctx.fillRect(x + 40 + scan * (tw - 80), y + 690, 5, 90)
    ctx.restore()
  }
  ctx.textAlign = 'center'
  ctx.fillStyle = '#2b2418'
  ctx.font = `800 54px ${f.titre}`
  ctx.fillText('ADMIT ONE', x + tw / 2, coupe + 120)
  ctx.font = `500 24px ${f.mono}`
  ctx.fillStyle = '#8a7a58'
  ctx.fillText('REGARDÉ JUSQU’AU BOUT · ENTRACTE', x + tw / 2, coupe + 175)
}

function polaroid(sc: Scene) {
  const { ctx, b, p, f, t } = sc
  ctx.fillStyle = '#d8c7a6'
  ctx.fillRect(0, 0, W, H)
  grain(ctx, 0.08, 7)
  const e = sortie(prog(t, 0, 0.8))
  const rot = ((hachage(b.title_id) % 7) - 3) / 60
  const choc = prog(t, 3.0, 3.25)
  const secousse = choc > 0 && choc < 1 ? Math.sin(choc * 40) * 8 * (1 - choc) : 0
  const pw = 700
  const ph = 900
  ctx.save()
  ctx.translate(W / 2 + secousse, 600 - (1 - e) * 900)
  ctx.rotate(rot + (1 - e) * 0.5)
  ctx.shadowColor = 'rgba(40,25,0,.45)'
  ctx.shadowBlur = 36 - (1 - e) * 20
  ctx.shadowOffsetY = 16 + (1 - e) * 40
  ctx.fillStyle = '#fbfaf5'
  ctx.fillRect(-pw / 2, -ph / 2, pw, ph)
  ctx.shadowColor = 'transparent'
  const dev = douce(prog(t, 0.8, 2.6))
  const zone: [number, number, number, number] = [-pw / 2 + 40, -ph / 2 + 40, pw - 80, 640]
  couvrir(sc, ...zone)
  ctx.save()
  ctx.globalCompositeOperation = 'saturation'
  ctx.fillStyle = `rgba(128,128,128,${1 - dev})`
  ctx.fillRect(...zone)
  ctx.restore()
  ctx.fillStyle = `rgba(32,38,48,${(1 - dev) * 0.96})`
  ctx.fillRect(...zone)
  ctx.fillStyle = `rgba(255,200,120,${0.12 + (1 - dev) * 0.1})`
  ctx.fillRect(...zone)
  ctx.fillStyle = '#2d2a26'
  ctx.textAlign = 'center'
  ctx.font = `italic 600 46px ${f.serif}`
  const n = couper(b.title, pw - 100, (s) => ctx.measureText(s).width).length
  lignes(ctx, tape(b.title, prog(t, 1.6, 2.6)), 0, ph / 2 - 130 + (n === 1 ? 25 : 0), pw - 100, 50, 2)
  ctx.fillStyle = 'rgba(255,240,200,.75)'
  ctx.save()
  ctx.translate(0, -ph / 2)
  ctx.rotate(-0.04)
  ctx.fillRect(-110, -26, 220 * sortie(prog(t, 0.6, 0.9)), 56)
  ctx.restore()
  ctx.restore()
  const s = prog(t, 2.9, 3.15)
  if (s > 0) {
    const k = 1 + (1 - sortie(s)) * 1.8
    const tp = tampon(b)
    ctx.save()
    ctx.translate(820, 1120)
    ctx.rotate(-0.18)
    ctx.scale(k, k)
    ctx.strokeStyle = p.ac2
    ctx.fillStyle = p.ac2
    ctx.lineWidth = 6
    ctx.globalAlpha = 0.9 * s
    for (const r of [130, 112]) {
      ctx.beginPath()
      ctx.arc(0, 0, r, 0, Math.PI * 2)
      ctx.stroke()
    }
    ctx.textAlign = 'center'
    ctx.font = `800 30px ${f.titre}`
    ctx.fillText(tp.haut, 0, -18)
    ctx.font = `900 44px ${f.titre}`
    ctx.fillText(tp.bas, 0, 28)
    ctx.font = `600 22px ${f.mono}`
    ctx.fillText(dateCourte(b), 0, 66)
    ctx.restore()
  }
  const d = prog(t, 3.4, 3.9)
  ctx.globalAlpha = d
  ctx.fillStyle = '#3b2f1e'
  ctx.textAlign = 'left'
  ctx.font = `500 30px ${f.serif}`
  ctx.fillText(detailsAnim(b), 90, 1180 + (1 - d) * 20)
  ctx.globalAlpha = d * 0.6
  ctx.font = `600 24px ${f.mono}`
  ctx.fillText('ENTRACTE', 90, 1240)
  ctx.globalAlpha = 1
}

function pellicule(sc: Scene) {
  const { ctx, b, p, f, t } = sc
  ctx.fillStyle = p.bg
  ctx.fillRect(0, 0, W, H)
  const sx = 120
  const sw = 420
  const pas = 590
  const arret = sortie(prog(t, 0, 2.0))
  const decalage = (1 - arret) * pas * 6
  ctx.fillStyle = '#0b0b0b'
  ctx.fillRect(sx, 0, sw, H)
  ctx.fillStyle = p.bg
  for (let y = -56 + (decalage % 56); y < H; y += 56) {
    rr(ctx, sx + 16, y, 30, 36, 6)
    ctx.fill()
    rr(ctx, sx + sw - 46, y, 30, 36, 6)
    ctx.fill()
  }
  const fx = sx + 64
  const fw = sw - 128
  const fh = 470
  for (let k = -2; k <= 8; k++) {
    const fy = 330 - k * pas + decalage
    if (fy > H || fy + fh < 0) continue
    ctx.save()
    rr(ctx, fx, fy, fw, fh, 6)
    ctx.clip()
    couvrir(sc, fx, fy, fw, fh)
    if (!(Math.abs(fy - 330) < 20 && t > 2)) {
      ctx.fillStyle = 'rgba(0,0,0,.6)'
      ctx.fillRect(fx, fy, fw, fh)
    }
    ctx.restore()
  }
  if (t < 2) {
    ctx.save()
    ctx.globalAlpha = (1 - arret) * 0.6
    ctx.fillStyle = p.bg
    ctx.fillRect(sx, 0, sw, H)
    ctx.restore()
  }
  ctx.fillStyle = '#e0a63a'
  ctx.font = `700 20px ${f.mono}`
  ctx.save()
  ctx.translate(sx + 8, 300 + (decalage % pas))
  ctx.rotate(Math.PI / 2)
  ctx.textAlign = 'left'
  ctx.fillText(`ENTRACTE 400  ▸ ${String((hachage(b.title_id) % 90) + 10)}A`, 0, 0)
  ctx.restore()
  const tx = 600
  const a1 = sortie(prog(t, 1.8, 2.3))
  const a2 = sortie(prog(t, 2.2, 3.0))
  const a3 = sortie(prog(t, 2.8, 3.4))
  const a4 = prog(t, 3.3, 3.8)
  ctx.textAlign = 'left'
  ctx.globalAlpha = a1
  ctx.fillStyle = p.ac
  ctx.font = `800 30px ${f.mono}`
  ctx.fillText(couper(etiquetteBadge(b).toUpperCase(), 440, (s) => ctx.measureText(s).width, 1)[0], tx - (1 - a1) * 40, 360)
  ctx.save()
  ctx.globalAlpha = a2
  ctx.beginPath()
  ctx.rect(tx - 20, 380, 480 * a2, 200)
  ctx.clip()
  ctx.fillStyle = p.fg
  ctx.font = `italic 400 170px ${f.serif}`
  ctx.fillText('Fin.', tx - 8, 540)
  ctx.restore()
  ctx.globalAlpha = a3
  ctx.fillStyle = p.fg
  ctx.font = `800 54px ${f.titre}`
  const y = lignes(ctx, b.title, tx, 640 + (1 - a3) * 30, 420, 60, 3)
  ctx.globalAlpha = a4 * 0.75
  ctx.font = `500 30px ${f.texte}`
  ctx.fillText(detailsAnim(b), tx, y + 60)
  ctx.fillText(dateBadge(b.obtained_at), tx, y + 104)
  ctx.globalAlpha = a4 * 0.45
  ctx.font = `600 24px ${f.mono}`
  ctx.fillText('ENTRACTE', tx, H - 60)
  ctx.globalAlpha = 1
  const eclat = t < 2.2 ? 0.08 + 0.08 * Math.sin(t * 90) : 0.03 + 0.03 * Math.sin(t * 50)
  ctx.fillStyle = `rgba(255,240,210,${Math.max(0, eclat)})`
  ctx.fillRect(0, 0, W, H)
  grain(ctx, 0.05, Math.floor(t * 24) + 3)
  ctx.save()
  ctx.globalAlpha = 0.25
  ctx.fillStyle = '#fff'
  ctx.fillRect(hachage(`r${Math.floor(t * 12)}`) % W, 0, 2, H)
  ctx.restore()
}

function generique(sc: Scene) {
  const { ctx, b, p, f, t } = sc
  ctx.fillStyle = '#050505'
  ctx.fillRect(0, 0, W, H)
  const film = b.kind === 'movie'
  const credits: [string, string][] = [
    ['Un visionnage de', 'toi, jusqu’au bout'],
    [film ? 'Durée' : 'Épisodes', film ? duree(b) : `${b.episodes ?? 0} regardés`],
    [film ? 'Sorti en' : 'Temps passé', film ? String(b.year ?? '—') : duree(b)],
    ['Générique', 'passé avec le bouton'],
    ['Pop-corn', 'non fourni'],
    ['Terminé le', dateBadge(b.obtained_at)],
    ['Projection', 'Entracte'],
  ]
  const defil = douce(prog(t, 0, 3.6))
  const y0 = H + 40 - defil * ((credits.length + 2) * 110 + 300)
  ctx.textAlign = 'center'
  ctx.globalAlpha = 1 - prog(t, 2.9, 3.4)
  ctx.fillStyle = p.ac
  ctx.font = `800 40px ${f.mono}`
  ctx.fillText(etiquetteBadge(b).toUpperCase(), W / 2, y0)
  credits.forEach(([k, v], i) => {
    const y = y0 + (i + 2) * 110
    ctx.fillStyle = '#888'
    ctx.font = `500 26px ${f.mono}`
    ctx.fillText(k.toUpperCase(), W / 2, y)
    ctx.fillStyle = '#eee'
    ctx.font = `600 40px ${f.serif}`
    ctx.fillText(v, W / 2, y + 46)
  })
  ctx.globalAlpha = 1
  const fin = sortie(prog(t, 3.3, 4.4))
  if (fin > 0) {
    ctx.save()
    ctx.globalAlpha = fin
    const aw = 380 + fin * 60
    const ah = aw * 1.5
    const ax = (W - aw) / 2
    const ay = 140 + (1 - fin) * 80
    ctx.shadowColor = p.ac
    ctx.shadowBlur = 60 * fin
    rr(ctx, ax, ay, aw, ah, 18)
    ctx.fillStyle = '#000'
    ctx.fill()
    ctx.shadowBlur = 0
    ctx.save()
    rr(ctx, ax, ay, aw, ah, 18)
    ctx.clip()
    couvrir(sc, ax, ay, aw, ah)
    ctx.restore()
    ctx.textAlign = 'center'
    ctx.fillStyle = '#fff'
    ctx.font = `800 60px ${f.titre}`
    const y = lignes(ctx, b.title, W / 2, ay + ah + 100, W - 160, 64)
    ctx.fillStyle = p.ac
    ctx.font = `700 30px ${f.mono}`
    ctx.fillText(etiquetteBadge(b).toUpperCase(), W / 2, y + 60)
    ctx.fillStyle = '#999'
    ctx.font = `500 28px ${f.texte}`
    ctx.fillText([detailsAnim(b), dateBadge(b.obtained_at)].filter(Boolean).join(' · '), W / 2, y + 108)
    ctx.restore()
  }
  const fondu = prog(t, 3.0, 3.4) * (1 - prog(t, 3.4, 3.8))
  ctx.fillStyle = `rgba(0,0,0,${fondu})`
  ctx.fillRect(0, 0, W, H)
}

function amorce(sc: Scene) {
  const { ctx, b, p, f, t } = sc
  if (t < 3) {
    const n = 3 - Math.floor(t)
    const fr = t % 1
    const cx = W / 2
    const cy = H / 2
    ctx.fillStyle = '#d9d2c4'
    ctx.fillRect(0, 0, W, H)
    ctx.fillStyle = '#a79f8f'
    ctx.beginPath()
    ctx.moveTo(cx, cy)
    ctx.arc(cx, cy, 900, -Math.PI / 2, -Math.PI / 2 + fr * Math.PI * 2)
    ctx.closePath()
    ctx.fill()
    ctx.strokeStyle = '#222'
    ctx.lineWidth = 6
    ctx.beginPath()
    ctx.moveTo(0, cy)
    ctx.lineTo(W, cy)
    ctx.moveTo(cx, 0)
    ctx.lineTo(cx, H)
    ctx.stroke()
    ctx.lineWidth = 10
    for (const r of [300, 360]) {
      ctx.beginPath()
      ctx.arc(cx, cy, r, 0, Math.PI * 2)
      ctx.stroke()
    }
    ctx.fillStyle = '#111'
    ctx.font = `700 380px ${f.serif}`
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'
    ctx.fillText(String(n), cx, cy + 20)
    ctx.textBaseline = 'alphabetic'
    grain(ctx, 0.12, Math.floor(t * 24) + 1)
    ctx.fillStyle = `rgba(0,0,0,${0.15 + 0.1 * Math.sin(t * 70)})`
    ctx.fillRect(0, 0, W, H)
    ctx.fillStyle = '#111'
    ctx.font = `700 28px ${f.mono}`
    ctx.fillText(couper(b.title.toUpperCase(), W - 120, (s) => ctx.measureText(s).width, 1)[0], cx, 80)
    return
  }
  const e = sortie(prog(t, 3.0, 3.8))
  ctx.fillStyle = p.bg
  ctx.fillRect(0, 0, W, H)
  ctx.save()
  ctx.beginPath()
  ctx.arc(W / 2, H / 2, 40 + e * 1000, 0, Math.PI * 2)
  ctx.clip()
  couvrir(sc, 0, 0, W, H)
  const g = ctx.createLinearGradient(0, 600, 0, H)
  g.addColorStop(0, 'rgba(0,0,0,0)')
  g.addColorStop(1, 'rgba(0,0,0,0.94)')
  ctx.fillStyle = g
  ctx.fillRect(0, 0, W, H)
  ctx.restore()
  const a = sortie(prog(t, 3.5, 4.2))
  const c = rebond(prog(t, 3.9, 4.5))
  ctx.textAlign = 'center'
  ctx.globalAlpha = a
  ctx.fillStyle = '#fff'
  ctx.font = `900 84px ${f.titre}`
  const y = lignes(ctx, b.title, W / 2, 1010 + (1 - a) * 40, W - 140, 88)
  ctx.globalAlpha = 1
  if (c > 0) {
    ctx.save()
    ctx.translate(W / 2, y + 80)
    ctx.scale(c, c)
    ctx.font = `800 32px ${f.titre}`
    const lab = etiquetteBadge(b).toUpperCase()
    const lw = ctx.measureText(lab).width + 50
    ctx.fillStyle = p.ac
    rr(ctx, -lw / 2, -36, lw, 60, 30)
    ctx.fill()
    ctx.fillStyle = '#fff'
    ctx.fillText(lab, 0, 6)
    ctx.restore()
  }
  const d = prog(t, 4.3, 4.8)
  ctx.globalAlpha = d * 0.75
  ctx.fillStyle = '#fff'
  ctx.font = `500 30px ${f.texte}`
  ctx.fillText([detailsAnim(b), dateBadge(b.obtained_at)].filter(Boolean).join(' · '), W / 2, y + 160)
  ctx.globalAlpha = d * 0.5
  ctx.font = `600 24px ${f.mono}`
  ctx.fillText('ENTRACTE', W / 2, H - 40)
  ctx.globalAlpha = 1
}

const DESSINS: Record<StyleBadge, (sc: Scene) => void> = { ticket, polaroid, pellicule, generique, amorce }

export function dessinerBadge(ctx: Ctx, style: StyleBadge, b: FilmBadge, img: Affiche | null, p: Palette, f: Polices, t: number) {
  ctx.save()
  ctx.textBaseline = 'alphabetic'
  ctx.globalAlpha = 1
  DESSINS[style]({ ctx, b, img, p, f, t: Math.min(Math.max(0, t), DUREES[style]) })
  ctx.restore()
}
