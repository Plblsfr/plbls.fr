export type DrawPhase = 'lobby' | 'choosing' | 'drawing' | 'turn_end' | 'ended'

export interface DrawPlayer { id: string; pseudo: string; points: number; turn_points: number; connected: boolean; spectator: boolean; found: boolean }
export interface DrawSettings { rounds: number; turn_secs: number; word_choices: number; max_players: number; lang: string; custom_only: boolean; custom_count: number }
export interface DrawState {
  rev: number; server_id: string; host: string; phase: DrawPhase; settings: DrawSettings; players: DrawPlayer[]
  round: number; drawer: string; deadline: number | null; mask: string; word_len: number; reveal: string | null; reason: string
  draw_seq: number; reports: number; skip_votes: number; winners: string[]
}
export interface Trace { id: string; tool: 'pen' | 'eraser' | 'fill'; color: string; size: number; pts: [number, number][] }
export type DrawOp = ({ op: 'stroke' } & Trace) | { op: 'undo' } | { op: 'clear' }
export interface DrawPrivate { word?: string; choices?: string[] }
export interface DrawView { state: DrawState | null; now: number; me?: DrawPrivate; drawing?: { seq: number; strokes: Trace[] } }
export interface ChatLine { kind: 'guess' | 'chat' | 'found' | 'close' | 'chat_private'; from: string; pseudo: string; text: string; at: number }

export const CANVAS_W = 1000
export const CANVAS_H = 750
export const PALETTE = ['#101010', '#7a7a7a', '#ffffff', '#c5372f', '#d9822b', '#e8c93a', '#2f8a4e', '#4a90d9', '#7b4fb0', '#d9547a', '#8a5a3c']
export const SIZES = [4, 10, 20, 36]
export const ERASER_COLOR = '#ffffff'
export const MAX_CHAT = 100
export const BATCH = 8

export function applyOp(strokes: Trace[], op: DrawOp): boolean {
  if (op.op === 'clear') { strokes.length = 0; return false }
  if (op.op === 'undo') { strokes.pop(); return false }
  const cur = strokes.find((s) => s.id === op.id)
  if (cur) { cur.pts.push(...op.pts); return true }
  strokes.push({ id: op.id, tool: op.tool, color: op.color, size: op.size, pts: [...op.pts] })
  return true
}

export function stale(cur: number | undefined, rev: number): boolean {
  return cur !== undefined && rev < cur
}

export function remainingMs(deadline: number | null, offset: number, now: number): number {
  return deadline === null ? 0 : Math.max(0, deadline - (now + offset))
}

export function formatClock(ms: number): string {
  const s = Math.ceil(ms / 1000)
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

export function ranking(players: DrawPlayer[]): DrawPlayer[] {
  return players.filter((p) => !p.spectator || p.points > 0).sort((a, b) => b.points - a.points)
}

export function statusOf(p: DrawPlayer, s: DrawState): string {
  if (!p.connected) return 'absent'
  if (p.spectator) return 'spectateur'
  if (s.phase === 'drawing' || s.phase === 'choosing' || s.phase === 'turn_end') {
    if (p.id === s.drawer) return 'dessine'
    if (p.found) return 'trouvé'
  }
  return ''
}

export function chatText(l: ChatLine, meId: string): string {
  if (l.kind === 'found') return `${l.from === meId ? 'Tu as' : l.pseudo + ' a'} trouvé !`
  if (l.kind === 'close') return 'Tu es proche !'
  return `${l.pseudo} : ${l.text}`
}

export const REASONS: Record<string, string> = {
  time: 'Temps écoulé', all_found: 'Tout le monde a trouvé', drawer_left: 'Le dessinateur est parti', skipped: 'Tour passé',
}
