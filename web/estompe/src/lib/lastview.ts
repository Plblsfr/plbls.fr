/** Dernière vue ouverte (serveur/salon ou message privé), propre au compte : l'app mobile et la PWA s'y rouvrent (EST-28). */
const KEY = 'estompe:lastview'

export const saveLastView = (userId: string, href: string) => {
  try { localStorage.setItem(KEY, JSON.stringify({ userId, href })) } catch { /* stockage indisponible : ignoré */ }
}

export const clearLastView = () => {
  try { localStorage.removeItem(KEY) } catch { /* ignoré */ }
}

/** Conversation de la dernière vue (« c:salon » ou « d:message privé »), ou null : destination proposée à l'envoi d'un replay. */
export function lastViewConv(userId: string, servers: { id: string }[], dms: { id: string }[]): string | null {
  const [kind, id, channel] = (lastViewHref(userId, servers, dms) ?? '').replace(/^#\//, '').split('/').map(decodeURIComponent)
  if (kind === 'dm') return `d:${id}`
  return kind === 's' && channel ? `c:${channel}` : null
}

/** Adresse à rouvrir, ou null (rien de mémorisé, autre compte, serveur ou message privé disparu). */
export function lastViewHref(userId: string, servers: { id: string }[], dms: { id: string }[]): string | null {
  let saved: { userId?: string; href?: string } = {}
  try { saved = JSON.parse(localStorage.getItem(KEY) ?? '{}') } catch { /* ignoré */ }
  if (saved.userId !== userId || !saved.href) return null
  const [kind, id] = saved.href.replace(/^#\//, '').split('/').slice(0, 2).map(decodeURIComponent)
  const list = kind === 's' ? servers : kind === 'dm' ? dms : []
  return list.some((x) => x.id === id) ? saved.href : null
}
