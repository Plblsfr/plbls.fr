import { marked, type Tokens, type TokenizerAndRendererExtension } from 'marked'
import DOMPurify from 'dompurify'

/** Pseudos mentionnables (en minuscules) pendant un rendu : seuls ceux-là sont mis en évidence. */
let knownPseudos: Set<string> = new Set()
let knownEmojis: Set<string> = new Set()

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

// ||texte|| → spoiler
const spoiler: TokenizerAndRendererExtension = {
  name: 'spoiler',
  level: 'inline',
  start: (src) => src.indexOf('||'),
  tokenizer(src) {
    const m = /^\|\|(?=\S)([\s\S]+?)(?<=\S)\|\|/.exec(src)
    if (m) return { type: 'spoiler', raw: m[0], tokens: this.lexer.inlineTokens(m[1]) }
  },
  renderer(token) {
    return `<span class="spoiler">${this.parser.parseInline(token.tokens ?? [])}</span>`
  },
}

// @pseudo → mention, seulement pour un pseudo connu
const mention: TokenizerAndRendererExtension = {
  name: 'mention',
  level: 'inline',
  start: (src) => src.indexOf('@'),
  tokenizer(src) {
    // Noms de rôle : accents et espaces permis, isolés comme les pseudos (le serveur applique la même règle).
    const low = src.toLowerCase()
    const rich = [...knownPseudos].filter((p) => !/^[a-z0-9_.-]{3,20}$/.test(p) && low.startsWith('@' + p) && !/^[\p{L}\p{N}_]/u.test(src.slice(p.length + 1))).sort((a, b) => b.length - a.length)[0]
    if (rich) return { type: 'mention', raw: src.slice(0, rich.length + 1), text: src.slice(1, rich.length + 1) }
    const m = /^@([A-Za-z0-9_.-]{3,20})/.exec(src)
    if (!m) return
    const name = m[1].replace(/[.-]+$/, '')
    if (name.length < 3 || !knownPseudos.has(name.toLowerCase())) return
    return { type: 'mention', raw: '@' + name, text: name }
  },
  renderer(token) {
    return `<span class="mention">@${escapeHtml(token.text as string)}</span>`
  },
}

// :nom: → emoji personnalisé, seulement pour un nom connu ; l'image est posée ensuite par l'interface (voir MessageItem)
const cemoji: TokenizerAndRendererExtension = {
  name: 'cemoji',
  level: 'inline',
  start: (src) => src.indexOf(':'),
  tokenizer(src) {
    const m = /^:([a-z0-9_]{2,32}):/i.exec(src)
    if (!m || !knownEmojis.has(m[1].toLowerCase())) return
    return { type: 'cemoji', raw: m[0], text: m[1].toLowerCase() }
  },
  renderer(token) {
    return `<span class="cemoji">:${escapeHtml(token.text as string)}:</span>`
  },
}

marked.use({
  gfm: true,
  breaks: true,
  extensions: [spoiler, mention, cemoji],
  renderer: {
    // Aucun HTML brut : il est affiché comme du texte.
    html: ({ text }: Tokens.HTML | Tokens.Tag) => escapeHtml(text),
    // Pas d'images distantes dans le texte (confidentialité) : un lien à la place.
    image: ({ href, text }: Tokens.Image) => `<a href="${escapeHtml(href)}">${escapeHtml(text || href)}</a>`,
    // Pas de titres dans un chat.
    heading({ tokens }: Tokens.Heading) {
      return `<p><strong>${this.parser.parseInline(tokens)}</strong></p>`
    },
    hr: () => '',
  },
})

const ALLOWED_TAGS = ['p', 'br', 'strong', 'em', 'del', 'code', 'pre', 'blockquote', 'ul', 'ol', 'li', 'a', 'span', 'input']
const ALLOWED_ATTR = ['href', 'class', 'rel', 'target', 'type', 'checked', 'disabled']

DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    node.setAttribute('target', '_blank')
    node.setAttribute('rel', 'noopener noreferrer nofollow ugc')
  }
  // Les seules classes autorisées sont celles que le rendu produit lui-même.
  if (node.hasAttribute('class') && !['spoiler', 'mention', 'cemoji'].includes(node.getAttribute('class') ?? '')) {
    node.removeAttribute('class')
  }
  if (node.tagName === 'INPUT' && node.getAttribute('type') !== 'checkbox') node.remove()
})

/** Rend le markdown d'un message en HTML assaini. `pseudos` : pseudos des participants (mentions). */
export function renderMarkdown(src: string, pseudos: Iterable<string> = [], emojiNames: Iterable<string> = []): string {
  knownPseudos = new Set([...pseudos].map((p) => p.toLowerCase()))
  knownEmojis = new Set(emojiNames)
  const html = marked.parse(src, { async: false }) as string
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOWED_URI_REGEXP: /^(?:https?:|mailto:)/i,
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
  })
}

const URL_RE = /\bhttps?:\/\/[^\s<>()\]"']+/i

/** Premier lien http(s) du texte, hors blocs de code, pour l'aperçu. */
export function firstLink(src: string): string | null {
  const noCode = src.replace(/```[\s\S]*?```/g, '').replace(/`[^`\n]*`/g, '')
  const m = URL_RE.exec(noCode)
  return m ? m[0].replace(/[.,;:!?]+$/, '') : null
}
