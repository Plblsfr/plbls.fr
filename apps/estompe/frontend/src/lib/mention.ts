/** Mention en cours de frappe juste avant le curseur : « @ab » → { start, query: 'ab' }. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const m = /(^|[^A-Za-z0-9_.-])@([A-Za-z0-9_.-]{0,20})$/.exec(text.slice(0, caret))
  if (!m) return null
  return { start: caret - m[2].length - 1, query: m[2] }
}

/** Remplace la mention en cours par @pseudo suivi d'une espace. */
export function insertMention(text: string, caret: number, start: number, pseudo: string): { text: string; caret: number } {
  const inserted = `@${pseudo} `
  return { text: text.slice(0, start) + inserted + text.slice(caret), caret: start + inserted.length }
}

/** Pseudos proposés : ceux qui commencent par la saisie d'abord, puis ceux qui la contiennent ; jamais soi-même. */
export function suggest(query: string, pseudos: string[], me: string, max = 6): string[] {
  const q = query.toLowerCase()
  const list = pseudos.filter((p) => p !== me)
  const starts = list.filter((p) => p.toLowerCase().startsWith(q))
  const contains = list.filter((p) => !p.toLowerCase().startsWith(q) && p.toLowerCase().includes(q))
  return [...starts, ...contains].slice(0, max)
}

export const SEUIL_CONFIRMATION = 10

/** @everyone et @here cités hors code (bloc ou en ligne), comme le serveur. */
export function groupesCites(text: string): ('everyone' | 'here')[] {
  const sans = text.replace(/```[\s\S]*?(?:```|$)|`[^`\n]*`/g, ' ')
  const trouves = new Set([...sans.matchAll(/(?:^|[^A-Za-z0-9_.-])@(everyone|here)(?![\p{L}\p{N}_])/giu)].map((m) => m[1].toLowerCase() as 'everyone' | 'here'))
  return [...trouves]
}
