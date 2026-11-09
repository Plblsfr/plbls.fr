/** Sélection d'emojis courants (pas de dépendance, pas de requête réseau). */
export const EMOJIS = [
  '👍', '👎', '❤️', '😂', '😮', '😢', '🙏', '🔥',
  '🎉', '👏', '😍', '🤔', '😅', '😎', '🥳', '😭',
  '😡', '🤣', '😊', '😉', '🙂', '🙃', '😴', '🤯',
  '👀', '💯', '✅', '❌', '⭐', '💀', '🤝', '💪',
  '🙌', '👋', '🤷', '🫡', '😬', '🥲', '😇', '🤗',
  '🍕', '☕', '🍺', '🎮', '🎵', '🚀', '💡', '📌',
]
export const QUICK_REACTIONS = ['👍', '❤️', '😂', '😮', '😢', '🎉']

/** Texte de l'infobulle d'une réaction : « Toi, bob et 2 autres ont réagi avec 👍 ». */
export function reactionTitle(r: { emoji: string; users: { id: string; pseudo: string }[] }, meId: string, max = 3): string {
  const names = r.users.map((u) => (u.id === meId ? 'Toi' : u.pseudo))
  const shown = names.slice(0, max)
  const rest = names.length - shown.length
  if (rest > 0) shown.push(`${rest} autre${rest > 1 ? 's' : ''}`)
  const list = shown.length > 1 ? `${shown.slice(0, -1).join(', ')} et ${shown.at(-1)}` : (shown[0] ?? '')
  return `${list} ${names.length > 1 ? 'ont' : 'a'} réagi avec ${r.emoji}`
}

/** Un smiley du jeu de données (généré par scripts/gen-emojis.mjs) : [caractère, code, alias, mots-clés FR/EN sans accents, nom FR, catégorie]. */
export type EmojiRow = [char: string, code: string, aliases: string[], words: string, name: string, group: number]

export const EMOJI_GROUPS = [
  { icon: '😀', label: 'Smileys et émotions' },
  { icon: '👋', label: 'Personnes' },
  { icon: '🐻', label: 'Animaux et nature' },
  { icon: '🍔', label: 'Nourriture' },
  { icon: '⚽', label: 'Activités' },
  { icon: '🚗', label: 'Voyages et lieux' },
  { icon: '💡', label: 'Objets' },
  { icon: '❤️', label: 'Symboles' },
  { icon: '🏳️', label: 'Drapeaux' },
]

let rows: Promise<EmojiRow[]> | undefined
let byCode = new Map<string, string>()

/** Charge le jeu de données à la demande (dans son propre fichier, hors du chargement initial). */
export function loadEmojiRows(): Promise<EmojiRow[]> {
  rows ??= import('./emoji-data.json').then((m) => {
    const data = m.default as EmojiRow[]
    byCode = new Map(data.flatMap((r) => [r[1], ...r[2]].map((c) => [c, r[0]] as const)).reverse())
    return data
  }, (e) => {
    rows = undefined
    throw e
  })
  return rows
}

/** Smiley d'un code (« joy »), une fois le jeu de données chargé. */
export const emojiByCode = (code: string): string | undefined => byCode.get(code.toLowerCase())

const fold = (s: string) => s.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')

/** Recherche par nom, code (« :joy: » ou « joy ») et mots-clés ; les codes qui commencent par la saisie passent en premier. */
export function searchEmojis(data: EmojiRow[], query: string, max = 200): EmojiRow[] {
  const q = fold(query.trim().replace(/^:|:$/g, ''))
  if (!q) return []
  const scored: [number, EmojiRow][] = []
  for (const r of data) {
    const codes = [r[1], ...r[2]]
    let s = 0
    if (codes.includes(q)) s = 4
    else if (codes.some((c) => c.startsWith(q))) s = 3
    else if (r[3].split(' ').some((w) => w.startsWith(q))) s = 2
    else if (codes.some((c) => c.includes(q)) || fold(r[4]).includes(q) || r[3].includes(q)) s = 1
    if (s) scored.push([s, r])
  }
  return scored.sort((a, b) => b[0] - a[0]).slice(0, max).map((x) => x[1])
}

/** Vrai si la position est dans un bloc de code, du code en ligne ou une URL (où les codes ne sont jamais convertis). */
export function inCodeOrUrl(text: string, pos: number): boolean {
  const before = text.slice(0, pos)
  if ((before.match(/```/g)?.length ?? 0) % 2) return true
  const line = before.slice(before.lastIndexOf('\n') + 1)
  if ((line.replace(/```/g, '').match(/`/g)?.length ?? 0) % 2) return true
  return /\S*:\/\/\S*$/.test(before)
}

/** Code en cours de frappe juste avant le curseur : « :jo » → { start, query: 'jo' } (au moins deux caractères). */
export function codeAt(text: string, caret: number): { start: number; query: string } | null {
  const m = /(^|[^A-Za-z0-9:])(:([A-Za-z0-9_+-]{2,}))$/.exec(text.slice(0, caret))
  if (!m) return null
  const start = caret - m[2].length
  return inCodeOrUrl(text, start) ? null : { start, query: m[3] }
}

/** Remplace « :nom: » (second « : » juste tapé avant le curseur) par le smiley ; null si rien à convertir, nom inconnu ou réservé. */
export function completeCode(text: string, caret: number, find: (name: string) => string | undefined): { text: string; caret: number } | null {
  const m = /(^|[^A-Za-z0-9])(:([A-Za-z0-9_+-]{2,}):)$/.exec(text.slice(0, caret))
  if (!m) return null
  const start = caret - m[2].length
  const e = inCodeOrUrl(text, start) ? undefined : find(m[3])
  return e ? { text: text.slice(0, start) + e + text.slice(caret), caret: start + e.length } : null
}

const RECENT = 'estompe.emoji.recent'
const RECENT_MAX = 32

export function loadRecents(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT) ?? '[]')
    return Array.isArray(v) ? v.filter((x) => typeof x === 'string').slice(0, RECENT_MAX) : []
  } catch { return [] }
}

/** Place le smiley en tête des récents et renvoie la nouvelle liste. */
export function pushRecent(char: string): string[] {
  const l = [char, ...loadRecents().filter((x) => x !== char)].slice(0, RECENT_MAX)
  try { localStorage.setItem(RECENT, JSON.stringify(l)) } catch { /* stockage indisponible */ }
  return l
}
