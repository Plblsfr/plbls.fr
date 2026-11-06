import { api } from '../../lib/api'
import { applyOp, MAX_CHAT, stale, type ChatLine, type DrawOp, type DrawPrivate, type DrawState, type DrawView, type Trace } from './draw'

export interface Game {
  state: DrawState
  offset: number
  me: DrawPrivate
  chat: ChatLine[]
  strokes: Trace[]
  seq: number
  v: number
  epoch: number
  hidden: boolean
}

export class DrawGames {
  games = $state<Record<string, Game>>({})
  private queue: Promise<unknown> = Promise.resolve()

  setState(cid: string, d: { state: DrawState | null; now: number }) {
    if (!d.state) { delete this.games[cid]; return }
    const prev = this.games[cid]
    if (prev && stale(prev.state.rev, d.state.rev)) return
    const offset = d.now - Date.now()
    if (!prev) {
      this.games[cid] = { state: d.state, offset, me: {}, chat: [], strokes: [], seq: d.state.draw_seq, v: 0, epoch: 0, hidden: false }
      void this.load(cid)
      return
    }
    const turnStart = d.state.phase === 'choosing' && prev.state.phase !== 'choosing'
    const g = prev
    g.state = d.state
    g.offset = offset
    if (turnStart || d.state.phase === 'lobby') { g.strokes = []; g.seq = d.state.draw_seq; g.epoch++; g.v++ }
    if (d.state.phase !== 'choosing' && d.state.phase !== 'drawing') g.me = {}
    if (d.state.phase === 'lobby') g.chat = []
  }

  setPrivate(cid: string, d: DrawPrivate & { rev: number }) {
    const g = this.games[cid]
    if (g && !stale(g.state.rev, d.rev)) g.me = { word: d.word, choices: d.choices }
  }

  addChat(cid: string, line: ChatLine) {
    const g = this.games[cid]
    if (!g) return
    g.chat.push(line)
    if (g.chat.length > MAX_CHAT) g.chat.shift()
  }

  applyOps(cid: string, d: { seq: number; ops: DrawOp[] }, meId: string) {
    const g = this.games[cid]
    if (!g || g.state.drawer === meId || d.seq <= g.seq) return
    if (d.seq !== g.seq + 1) { void this.load(cid); return }
    for (const op of d.ops) if (!applyOp(g.strokes, op)) g.epoch++
    g.seq = d.seq
    g.v++
  }

  apply(cid: string, op: DrawOp) {
    const g = this.games[cid]
    if (!g) return
    if (!applyOp(g.strokes, op)) g.epoch++
    g.v++
  }

  send(cid: string, op: DrawOp) {
    this.queue = this.queue.then(() => api.drawOp(cid, op)).catch(() => this.load(cid))
  }

  async load(cid: string) {
    let v: DrawView
    try { v = await api.draw(cid) } catch { return }
    if (!v.state) { delete this.games[cid]; return }
    const g = this.games[cid]
    if (!g) {
      this.games[cid] = { state: v.state, offset: v.now - Date.now(), me: v.me ?? {}, chat: [], strokes: v.drawing?.strokes ?? [], seq: v.drawing?.seq ?? 0, v: 0, epoch: 0, hidden: false }
      return
    }
    if (!stale(g.state.rev, v.state.rev)) { g.state = v.state; g.offset = v.now - Date.now() }
    g.me = v.me ?? {}
    g.strokes = v.drawing?.strokes ?? []
    g.seq = v.drawing?.seq ?? 0
    g.epoch++
    g.v++
  }

  drop(cid: string) { delete this.games[cid] }

  reloadAll() { for (const cid of Object.keys(this.games)) void this.load(cid) }
}

export const draw = new DrawGames()
