import { api } from '../../lib/api'
import type { Badge } from '../../lib/types'

/** Avatars des utilisateurs côtoyés : versions connues et images téléchargées (avec l'authentification) en URL locales. */
class Avatars {
  private versions = $state<Record<string, number>>({})
  private urls = $state<Record<string, string>>({})
  private serverVersions = $state<Record<string, Record<string, number>>>({})
  private pending = new Set<string>()
  private blobs = new Map<string, Blob>()

  async load() {
    try {
      this.versions = await api.avatars()
    } catch { /* les initiales restent affichées */ }
  }

  async loadServer(serverId: string) {
    try {
      this.serverVersions[serverId] = await api.serverAvatars(serverId)
    } catch { /* la photo globale reste affichée */ }
  }

  /** Évènement `server_avatar.updated` (version 0 = photo du serveur supprimée). */
  updateServer(serverId: string, userId: string, version: number) {
    const v = (this.serverVersions[serverId] ??= {})
    if (version > 0) v[userId] = version
    else delete v[userId]
  }

  /** Version de la photo propre au serveur, ou 0 : la photo globale s'applique. */
  serverVersion(userId: string, serverId?: string): number {
    return (serverId && this.serverVersions[serverId]?.[userId]) || 0
  }

  /** Évènement `avatar.updated` (version 0 = avatar supprimé). */
  update(userId: string, version: number) {
    if (version > 0) this.versions[userId] = version
    else delete this.versions[userId]
  }

  /** URL de l'avatar à afficher dans ce serveur (photo du serveur, sinon photo globale), ou chaîne vide (initiales). */
  src(userId: string, serverId?: string): string {
    const sv = this.serverVersion(userId, serverId)
    const own = sv ? this.urls[`${serverId}/${userId}:${sv}`] : ''
    if (own) return own
    const v = this.versions[userId]
    return v ? (this.urls[`${userId}:${v}`] ?? '') : ''
  }

  /** Image d'une URL de cet store, pour la passer à l'overlay sans la recharger (la CSP interdit le fetch des blob:). */
  blob(url: string): Blob | undefined {
    return this.blobs.get(url)
  }

  /** Lance le téléchargement si l'avatar n'est pas encore en mémoire. */
  ensure(userId: string, serverId?: string) {
    const sv = this.serverVersion(userId, serverId)
    if (sv) this.fetch(`${serverId}/${userId}:${sv}`, userId, serverId)
    const v = this.versions[userId]
    if (v) this.fetch(`${userId}:${v}`, userId)
  }

  private fetch(key: string, userId: string, serverId?: string) {
    if (this.urls[key] || this.pending.has(key)) return
    this.pending.add(key)
    api.avatarBlob(userId, serverId)
      .then((b) => { const u = URL.createObjectURL(b); this.blobs.set(u, b); this.urls[key] = u })
      .catch(() => {})
      .finally(() => this.pending.delete(key))
  }
}

export const avatars = new Avatars()

/** Icônes des serveurs : images téléchargées (avec l'authentification) en URL locales, par serveur et version. */
class ServerIcons {
  private urls = $state<Record<string, string>>({})
  private pending = new Set<string>()

  src(serverId: string, version: number): string {
    return version ? (this.urls[`${serverId}:${version}`] ?? '') : ''
  }

  ensure(serverId: string, version: number) {
    const key = `${serverId}:${version}`
    if (!version || this.urls[key] || this.pending.has(key)) return
    this.pending.add(key)
    api.serverIconBlobUrl(serverId)
      .then((u) => { this.urls[key] = u })
      .catch(() => {})
      .finally(() => this.pending.delete(key))
  }
}

export const serverIcons = new ServerIcons()

/** Icônes des badges : images téléchargées (avec l'authentification) en URL locales, par badge et version. */
class BadgeIcons {
  private urls = $state<Record<string, string>>({})
  private pending = new Set<string>()

  src(id: string, version: number): string {
    return version ? (this.urls[`${id}:${version}`] ?? '') : ''
  }

  ensure(id: string, version: number) {
    const key = `${id}:${version}`
    if (!version || this.urls[key] || this.pending.has(key)) return
    this.pending.add(key)
    api.badgeIconBlobUrl(id)
      .then((u) => { this.urls[key] = u })
      .catch(() => {})
      .finally(() => this.pending.delete(key))
  }
}

export const badgeIcons = new BadgeIcons()

/** Badges des utilisateurs affichés à côté de leur pseudo : une requête par utilisateur, mise en cache pour la session. */
class UserBadges {
  private lists = $state<Record<string, Badge[]>>({})
  private pending = new Set<string>()

  of(userId: string): Badge[] {
    return this.lists[userId] ?? []
  }

  ensure(userId: string) {
    if (this.lists[userId] || this.pending.has(userId)) return
    this.pending.add(userId)
    api.userBadges(userId)
      .then((l) => { this.lists[userId] = l })
      .catch(() => {})
      .finally(() => this.pending.delete(userId))
  }
}

export const userBadges = new UserBadges()

/** Recadrage choisi : `zoom` ≥ 1 ; `x` et `y` dans [-1, 1] déplacent la fenêtre d'un bord à l'autre de l'image (0 : centré). */
export interface Crop { zoom: number; x: number; y: number }

/** Carré source (en pixels de l'image de w × h) correspondant au recadrage. */
export function cropRect(w: number, h: number, { zoom, x, y }: Crop = { zoom: 1, x: 0, y: 0 }) {
  const size = Math.min(w, h) / Math.max(1, zoom)
  return { sx: (w - size) / 2 + (x * (w - size)) / 2, sy: (h - size) / 2 + (y * (h - size)) / 2, size }
}

/** Recadre une image en carré (centré, ou selon `crop`) et la réduit à 256 px (PNG, ou JPEG si elle reste trop lourde). */
export async function squareAvatar(file: Blob, side = 256, crop?: Crop): Promise<Blob> {
  const bmp = await createImageBitmap(file)
  const { sx, sy, size } = cropRect(bmp.width, bmp.height, crop)
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = side
  canvas.getContext('2d')!.drawImage(bmp, sx, sy, size, size, 0, 0, side, side)
  bmp.close()
  const toBlob = (type: string, q?: number) => new Promise<Blob | null>((r) => canvas.toBlob(r, type, q))
  const png = await toBlob('image/png')
  if (png && png.size <= 200 << 10) return png
  const jpg = await toBlob('image/jpeg', 0.85)
  if (!jpg) throw new Error('image illisible')
  return jpg
}
