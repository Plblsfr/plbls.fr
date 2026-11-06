import { Marked } from 'marked'
import DOMPurify from 'dompurify'
import { ApiError } from '../../lib/api'

export interface CairnSpace { key: string; name: string; archived: boolean }
export type TicketStatus = 'todo' | 'in_progress' | 'done' | 'cancelled'
export type TicketType = 'task' | 'bug' | 'idea'
export type TicketPriority = 'low' | 'normal' | 'high' | 'urgent'
export interface Ticket {
  key: string; space: string; title: string; description: string | null
  status: TicketStatus; type: TicketType; priority: TicketPriority
  assignee: string | null; labels: string[]; due: string | null
  version: number; created_by: string; created_at: string; updated_at: string; blocked: boolean
}
export type TicketPatch = Partial<Pick<Ticket, 'title' | 'description' | 'status' | 'type' | 'priority' | 'assignee' | 'labels' | 'due'>>
export interface CairnComment { id: string; author: string; content: string; created_at: string; updated_at: string }
export interface TicketEvent { kind: string; field: string | null; before: string | null; after: string | null; author: string; token: string | null; created_at: string }
export interface DepGraph { nodes: { key: string; title: string; status: TicketStatus; blocked: boolean }[]; edges: { from: string; to: string }[] }
export interface PageItem { id: string; parent_id: string | null; position: number; title: string; author: string; updated_at: string }
export interface CairnPage extends PageItem { space: string; content: string; version: number; created_at: string }
export interface PageVersion { number: number; title: string; author: string; created_at: string; content?: string }
export interface TrashItem { id: string; title: string; deleted_at: string }
export interface LinkedPage { id: string; title: string; space: string; origin: string }
export interface LinkedTicket { key: string; title: string; status: TicketStatus; blocked: boolean; origin: string }
export interface SearchHit { type: 'ticket' | 'page'; id: string; title: string; space: string; snippet: string }
export interface CairnUser { id: string; email: string }

export const STATUSES: { id: TicketStatus; label: string }[] = [
  { id: 'todo', label: 'À faire' }, { id: 'in_progress', label: 'En cours' }, { id: 'done', label: 'Fini' }, { id: 'cancelled', label: 'Annulé' },
]
export const TYPES: { id: TicketType; label: string }[] = [{ id: 'task', label: 'Tâche' }, { id: 'bug', label: 'Bug' }, { id: 'idea', label: 'Idée' }]
export const PRIORITIES: { id: TicketPriority; label: string }[] = [
  { id: 'low', label: 'Basse' }, { id: 'normal', label: 'Normale' }, { id: 'high', label: 'Haute' }, { id: 'urgent', label: 'Urgente' },
]
/** Teintes des badges `plbls-status` : le statut et la priorité se lisent d'un coup d'œil, comme les autres badges d'Estompe. */
export const STATUS_TONE: Record<TicketStatus, string> = { todo: '', in_progress: 'plbls-status--accent', done: 'plbls-status--positif', cancelled: 'plbls-status--negatif' }
export const PRIORITY_TONE: Record<TicketPriority, string> = { low: '', normal: '', high: 'plbls-status--accent', urgent: 'plbls-status--negatif' }
export const label = (list: { id: string; label: string }[], id: string) => list.find((x) => x.id === id)?.label ?? id

/** Message clair pour une erreur de l'API Cairn (relayée par le proxy). */
export function cairnError(e: unknown): string {
  if (!(e instanceof ApiError)) return String((e as Error)?.message ?? e)
  if (e.status === 503) return 'Cairn est injoignable pour le moment. Réessayez dans un instant.'
  if (e.status === 403) return "Action refusée par Cairn : vous n'avez pas le droit de la faire."
  // Cairn répond {"error": "message"} : le texte est dans `code` ; le proxy répond {error: code, message}.
  return e.code.includes(' ') ? e.code : e.message || e.code
}

/** Pages à plat → arbre (profondeur 5 max côté Cairn), dans l'ordre reçu. */
export interface PageNode extends PageItem { children: PageNode[] }
export function buildTree(pages: PageItem[]): PageNode[] {
  const nodes = new Map(pages.map((p) => [p.id, { ...p, children: [] as PageNode[] }]))
  const roots: PageNode[] = []
  for (const n of nodes.values()) {
    const parent = n.parent_id ? nodes.get(n.parent_id) : undefined
    ;(parent ? parent.children : roots).push(n)
  }
  return roots
}

/** Fil d'Ariane : la page et ses parents, de la racine à la page. */
export function pageTrail(pages: PageItem[], id: string): PageItem[] {
  const byId = new Map(pages.map((p) => [p.id, p]))
  const out: PageItem[] = []
  for (let p = byId.get(id); p && out.length < 10; p = p.parent_id ? byId.get(p.parent_id) : undefined) out.unshift(p)
  return out
}

const norm = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim()

/** Retire le premier titre `# …` du contenu quand il redit le titre de la page (l'en-tête l'affiche déjà). */
export function stripTitle(md: string, title: string): string {
  const m = /^\s*#[ \t]+(.+?)[ \t#]*(?:\r?\n|$)/.exec(md)
  if (!m) return md
  const a = norm(m[1]), b = norm(title)
  return a && b && (a.includes(b) || b.includes(a)) ? md.slice(m[0].length) : md
}

/** Champs que l'utilisateur a modifiés par rapport à la version qu'il a lue (seuls ceux-là sont envoyés : c'est la fusion). */
export function changedFields(base: Ticket, draft: TicketPatch): TicketPatch {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(draft)) {
    const b = base[k as keyof Ticket]
    if (JSON.stringify(b) !== JSON.stringify(v)) out[k] = v
  }
  return out as TicketPatch
}

/** Champs modifiés à la fois par l'utilisateur et par quelqu'un d'autre depuis sa lecture : ceux-là écraseraient l'autre. */
export function conflictFields(base: Ticket, fresh: Ticket, mine: TicketPatch): string[] {
  return Object.keys(mine).filter((k) => JSON.stringify(base[k as keyof Ticket]) !== JSON.stringify(fresh[k as keyof Ticket]))
}

/** Les images d'une page (`/api/files/{id}`) passent par le proxy d'Estompe. */
export function proxyImages(md: string, apiUrl: string): string {
  return md.replace(/\(\/api\/files\/([A-Za-z0-9-]+)\)/g, `(${apiUrl}/cairn/files/$1)`)
}

const doc = new Marked({ gfm: true, breaks: false })

/** Rend une page ou un ticket en HTML assaini. Les seules images admises sont celles du proxy d'Estompe. */
export function renderDoc(src: string, apiUrl: string): string {
  const prefix = `${apiUrl}/cairn/files/`
  const html = doc.parse(proxyImages(src, apiUrl), { async: false }) as string
  const clean = DOMPurify.sanitize(html, {
    ALLOWED_TAGS: ['p', 'br', 'strong', 'em', 'del', 'code', 'pre', 'blockquote', 'ul', 'ol', 'li', 'a', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'hr', 'table', 'thead', 'tbody', 'tr', 'th', 'td', 'img', 'input', 'span'],
    ALLOWED_ATTR: ['href', 'src', 'alt', 'title', 'type', 'checked', 'disabled'],
    ALLOWED_URI_REGEXP: /^(?:https?:|mailto:)/i,
    ALLOW_DATA_ATTR: false,
  })
  const tpl = document.createElement('template')
  tpl.innerHTML = clean
  tpl.content.querySelectorAll('img').forEach((img) => {
    if (!(img.getAttribute('src') ?? '').startsWith(prefix)) img.replaceWith(Object.assign(document.createElement('span'), { textContent: img.alt || '[image externe]' }))
  })
  const seen = new Map<string, number>()
  tpl.content.querySelectorAll('h1, h2, h3, h4').forEach((h) => {
    const base = 'cx-' + (norm(h.textContent ?? '').replace(/ /g, '-') || 'titre')
    const n = seen.get(base) ?? 0
    seen.set(base, n + 1)
    h.id = n ? `${base}-${n}` : base
  })
  tpl.content.querySelectorAll('a').forEach((a) => { a.target = '_blank'; a.rel = 'noopener noreferrer nofollow ugc' })
  tpl.content.querySelectorAll('input').forEach((i) => { if (i.type !== 'checkbox') i.remove() })
  return tpl.innerHTML
}

/** Sommaire d'une page rendue par `renderDoc` : les titres h2 à h4 et leur ancre. */
export function tocOf(html: string): { id: string; text: string; level: number }[] {
  const tpl = document.createElement('template')
  tpl.innerHTML = html
  return [...tpl.content.querySelectorAll('h2, h3, h4')].map((h) => ({ id: h.id, text: h.textContent ?? '', level: Number(h.tagName[1]) }))
}
