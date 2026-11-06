import type { AdminBadge, Badge } from '../../lib/types'

export type BadgeSort = 'order' | 'name' | 'holders' | 'created'
export const BADGE_SORTS: [BadgeSort, string][] = [['order', "Ordre d'affichage"], ['name', 'Nom'], ['holders', 'Nombre de détenteurs'], ['created', 'Date de création']]

const fold = (s: string) => s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()

/** Badges dont le nom ou la description contient `query` (sans tenir compte des accents ni de la casse), triés. */
export function filterBadges(list: AdminBadge[], query: string, sort: BadgeSort): AdminBadge[] {
  const q = fold(query.trim())
  const byName = (a: AdminBadge, b: AdminBadge) => a.name.localeCompare(b.name, 'fr')
  const cmp: Record<BadgeSort, (a: AdminBadge, b: AdminBadge) => number> = {
    order: (a, b) => a.sort_order - b.sort_order || byName(a, b),
    name: byName,
    holders: (a, b) => b.holders.length - a.holders.length || byName(a, b),
    created: (a, b) => b.created_at.localeCompare(a.created_at) || byName(a, b),
  }
  return list.filter((b) => !q || fold(b.name).includes(q) || fold(b.description).includes(q)).sort(cmp[sort])
}

const ICON_TYPES = ['image/svg+xml', 'image/webp', 'image/png', 'image/jpeg']
export const BADGE_ICON_ACCEPT = ICON_TYPES.join(',')
export const MAX_SVG_BYTES = 64 << 10

/** Message d'erreur si le fichier choisi ne peut pas servir d'icône, sinon chaîne vide. */
export function badgeIconProblem(f: { type: string; size: number }): string {
  if (!ICON_TYPES.includes(f.type)) return 'Format non pris en charge : SVG, WebP, PNG ou JPEG uniquement.'
  if (f.type === 'image/svg+xml' && f.size > MAX_SVG_BYTES) return 'SVG trop lourd (64 Ko maximum).'
  if (f.size > 5 << 20) return 'Image trop lourde.'
  return ''
}

/** Badges affichés à côté du pseudo (`max` au plus) et nombre de ceux qui restent cachés (« +N »). */
export function inlineBadges(list: Badge[], max = 2): { shown: Badge[]; extra: number } {
  return { shown: list.slice(0, max), extra: Math.max(0, list.length - max) }
}
