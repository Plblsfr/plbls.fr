/** Serveurs triés selon l'ordre choisi ; ceux qui n'y figurent pas (nouveaux) restent à la fin, dans leur ordre d'origine. */
export function orderServers<T extends { id: string }>(servers: T[], order: string[]): T[] {
  const rank = new Map(order.map((id, i) => [id, i]))
  return servers.map((s, i) => ({ s, k: rank.get(s.id) ?? order.length + i })).sort((a, b) => a.k - b.k).map((x) => x.s)
}

/** Nouvel ordre des identifiants après avoir déposé `from` à la place de `to`. */
export function moveServer(ids: string[], from: string, to: string): string[] {
  const i = ids.indexOf(from), j = ids.indexOf(to)
  if (i < 0 || j < 0 || i === j) return ids
  const next = ids.slice()
  next.splice(i, 1)
  next.splice(j, 0, from)
  return next
}

/** Élément voisin de `current` dans `ids` (-1 : précédent, 1 : suivant), en bouclant ; le premier si `current` est absent. */
export function adjacent(ids: string[], current: string, dir: -1 | 1): string | undefined {
  if (!ids.length) return undefined
  const i = ids.indexOf(current)
  return i < 0 ? ids[0] : ids[(i + dir + ids.length) % ids.length]
}
