/** Recherche de messages : aide à la saisie des filtres, surlignage et dernières recherches (par appareil). */

export type Scope = 'all' | 'server' | 'conv'
export interface SearchContext { pseudos: string[]; channels: string[]; servers: string[] }
export interface Suggestion { label: string; insert: string }

const FILTERS: [key: string, hint: string][] = [
  ['de', 'auteur'], ['dans', 'salon'], ['serveur', 'serveur'], ['mentionne', 'personne mentionnée'],
  ['avant', 'AAAA-MM-JJ'], ['après', 'AAAA-MM-JJ'], ['pendant', 'AAAA-MM-JJ'], ['a', 'fichier, image, vidéo, lien, sondage'], ['épinglé', 'oui'],
]
const HAS = ['fichier', 'image', 'vidéo', 'lien', 'sondage']

const ACCENTS = /[àâäéèêëîïôöùûüç]/g
const PLAIN: Record<string, string> = { à: 'a', â: 'a', ä: 'a', é: 'e', è: 'e', ê: 'e', ë: 'e', î: 'i', ï: 'i', ô: 'o', ö: 'o', ù: 'u', û: 'u', ü: 'u', ç: 'c' }
/** Minuscules sans accents, de même longueur que le texte (les positions restent valables). */
export const fold = (s: string) => s.toLowerCase().replace(ACCENTS, (c) => PLAIN[c])

const quote = (v: string) => (/\s/.test(v) ? `"${v}"` : v)

/** Propositions pour le dernier mot saisi : une clé de filtre (« de »), ou une valeur (« de:ali »). */
export function suggestions(input: string, ctx: SearchContext): Suggestion[] {
  const tok = input.split(/\s+/).at(-1) ?? ''
  if (!tok || tok.startsWith('"') || tok.startsWith('-')) return []
  const cut = tok.indexOf(':')
  const head = input.slice(0, input.length - tok.length)
  if (cut < 0) {
    return FILTERS.filter(([k]) => fold(k).startsWith(fold(tok))).map(([k, hint]) => ({ label: `${k}: ${hint}`, insert: `${head}${k}:` }))
  }
  const key = fold(tok.slice(0, cut))
  const val = fold(tok.slice(cut + 1).replace(/^["#@]/, ''))
  const pool: Record<string, string[]> = { de: ctx.pseudos, mentionne: ctx.pseudos, dans: ctx.channels, serveur: ctx.servers, a: HAS, epingle: ['oui'] }
  const prefix = tok.slice(0, cut + 1)
  return (pool[key] ?? [])
    .filter((v) => fold(v).startsWith(val) && fold(v) !== val)
    .slice(0, 8)
    .map((v) => ({ label: v, insert: `${head}${prefix}${key === 'dans' ? '#' : ''}${quote(v)} ` }))
}

/** Mots à surligner : le texte libre sans les filtres, les exclusions ni les guillemets. */
export function highlightTerms(input: string): string[] {
  const terms: string[] = []
  for (const t of input.match(/"[^"]*"|\S+/g) ?? []) {
    if (t.startsWith('-') || /^[^\s"]+:\S/.test(t) && FILTERS.some(([k]) => fold(k) === fold(t.slice(0, t.indexOf(':'))))) continue
    terms.push(...t.replace(/"/g, '').split(/\s+/))
  }
  // Racines : « réunions » surligne aussi « réunion ».
  return terms.map((t) => fold(t).replace(/[^\p{L}\p{N}]/gu, '')).filter((t) => t.length >= 2).map((t) => (t.length > 3 ? t.replace(/[sx]$/, '') : t))
}

export interface Piece { text: string; hit: boolean }

/** Extrait d'environ `width` caractères autour du premier terme trouvé, termes marqués. */
export function excerpt(content: string, terms: string[], width = 180): Piece[] {
  const plain = fold(content)
  const hits: [number, number][] = []
  for (const t of terms) for (let i = plain.indexOf(t); i >= 0; i = plain.indexOf(t, i + t.length)) hits.push([i, i + t.length])
  hits.sort((a, b) => a[0] - b[0])
  const merged: [number, number][] = []
  for (const h of hits) {
    const last = merged.at(-1)
    if (last && h[0] <= last[1]) last[1] = Math.max(last[1], h[1])
    else merged.push([...h])
  }
  const from = Math.max(0, (merged[0]?.[0] ?? 0) - Math.floor(width / 4))
  const to = Math.min(content.length, from + width)
  const out: Piece[] = []
  let at = from
  for (const [a, b] of merged) {
    if (b <= from || a >= to) continue
    if (a > at) out.push({ text: content.slice(at, a), hit: false })
    out.push({ text: content.slice(Math.max(a, at), Math.min(b, to)), hit: true })
    at = Math.min(b, to)
  }
  if (at < to) out.push({ text: content.slice(at, to), hit: false })
  if (from > 0) out.unshift({ text: '…', hit: false })
  if (to < content.length) out.push({ text: '…', hit: false })
  return out
}

const KEY = 'estompe:recherches'
const MAX_RECENT = 10

export function recentSearches(): string[] {
  try {
    const l = JSON.parse(localStorage.getItem(KEY) ?? '[]')
    return Array.isArray(l) ? l.filter((s) => typeof s === 'string') : []
  } catch { return [] }
}

export function rememberSearch(q: string): string[] {
  const l = [q, ...recentSearches().filter((s) => s !== q)].slice(0, MAX_RECENT)
  try { localStorage.setItem(KEY, JSON.stringify(l)) } catch { /* stockage indisponible : ignoré */ }
  return l
}

export function clearRecentSearches() {
  try { localStorage.removeItem(KEY) } catch { /* ignoré */ }
}
