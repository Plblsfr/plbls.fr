import { API_URL } from '../../lib/api'
import type { MusicMeta } from '../../lib/types'

/** Adresse de la pochette (servie par Estompe, qui la récupère sur YouTube une fois). */
export const coverUrl = (m: Pick<MusicMeta, 'cover'>) => (m.cover.startsWith('/') ? API_URL + m.cover : '')

/** « 3:25 » ou « 1:02:03 » ; vide si la durée est inconnue. */
export function clock(s: number): string {
  if (!(s > 0)) return ''
  const t = Math.floor(s)
  const h = Math.floor(t / 3600)
  const mm = String(Math.floor(t / 60) % 60).padStart(h ? 2 : 1, '0')
  const ss = String(t % 60).padStart(2, '0')
  return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`
}

/** Couleur moyenne (`rgb(r, g, b)`) de pixels RGBA ; les pixels quasi transparents sont ignorés. */
export function averageColor(px: ArrayLike<number>): string {
  let r = 0, g = 0, b = 0, n = 0
  for (let i = 0; i + 3 < px.length; i += 4) {
    if (px[i + 3] < 128) continue
    r += px[i]; g += px[i + 1]; b += px[i + 2]; n++
  }
  return n ? `rgb(${Math.round(r / n)}, ${Math.round(g / n)}, ${Math.round(b / n)})` : ''
}

/** Zone carrée de la pochette dans une miniature YouTube 4:3 (hqdefault) : elle est entourée de bandes noires (12,5 % en haut et en bas de la hauteur). */
export function coverCrop(w: number, h: number): { x: number; y: number; side: number } {
  const side = Math.floor(Math.min(w, h * 0.75))
  return { x: Math.floor((w - side) / 2), y: Math.floor((h - side) / 2), side }
}

/** Charge la pochette, la recadre en carré sans les bandes noires et en tire la couleur moyenne ; `src` sert d'image (URL d'objet). */
export async function loadCover(url: string): Promise<{ src: string; color: string }> {
  const blob = await (await fetch(url, { credentials: 'include' })).blob()
  try {
    const img = await createImageBitmap(blob)
    const { x, y, side } = coverCrop(img.width, img.height)
    const full = document.createElement('canvas')
    full.width = full.height = side
    full.getContext('2d')!.drawImage(img, x, y, side, side, 0, 0, side, side)
    const c = document.createElement('canvas')
    c.width = c.height = 16
    const ctx = c.getContext('2d')!
    ctx.drawImage(full, 0, 0, 16, 16)
    const square = await new Promise<Blob | null>((ok) => full.toBlob(ok, 'image/jpeg', 0.92))
    return { src: URL.createObjectURL(square ?? blob), color: averageColor(ctx.getImageData(0, 0, 16, 16).data) }
  } catch {
    return { src: URL.createObjectURL(blob), color: '' }
  }
}
