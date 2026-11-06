import { debug } from './debuglog.svelte'
export interface RealtimeEvent { type: string; data: any }
export type RealtimeStatus = 'connecting' | 'open' | 'closed'

export interface RealtimeOptions {
  /** Obtient un ticket à usage unique (POST /ws-ticket). */
  getTicket: () => Promise<string>
  /** URL ws(s) du endpoint /ws, sans ticket. */
  url: string
  onEvent: (e: RealtimeEvent) => void
  onStatus?: (s: RealtimeStatus) => void
  /** Appelé après une reconnexion : des évènements ont pu être manqués, il faut recharger l'état. */
  onReconnected?: () => void
  /** Le serveur a fermé avec 4401 : la session a expiré, il faut se reconnecter. */
  onSessionExpired?: () => void
  /** Le serveur a fermé avec 4403 : l'accès à Estompe a été retiré, inutile de se reconnecter. */
  onAccessRevoked?: () => void
  WebSocketImpl?: typeof WebSocket
}

const PING_EVERY = 10_000
const DEAD_AFTER = 25_000
const MAX_WAIT = 5_000

/** Client WebSocket : ticket, reconnexion (immédiate au premier essai, puis attente croissante plafonnée), reprise sur évènement réseau, ping applicatif, détection de connexion morte. */
export class Realtime {
  private ws: WebSocket | null = null
  private stopped = true
  private attempt = 0
  private everOpen = false
  private timers: ReturnType<typeof setTimeout>[] = []
  private ping: ReturnType<typeof setInterval> | null = null
  private lastSeen = 0
  private lostAt = 0
  private pending = false

  constructor(private o: RealtimeOptions) {}

  start() {
    if (!this.stopped) return
    this.stopped = false
    addEventListener('online', this.nudge)
    addEventListener('pageshow', this.nudge)
    document.addEventListener('visibilitychange', this.nudge)
    void this.connect()
  }

  /** Retour du réseau, de la veille ou du premier plan : on retente tout de suite, ou on vérifie que la connexion vit. */
  private nudge = () => {
    if (this.stopped || document.visibilityState === 'hidden') return
    if (this.pending) return
    if (this.ws) {
      if (this.ws.readyState === 1 && Date.now() - this.lastSeen > PING_EVERY) this.ws.send('{"type":"ping"}')
      return
    }
    this.clear()
    this.attempt = 0
    void this.connect()
  }

  stop() {
    this.stopped = true
    removeEventListener('online', this.nudge)
    removeEventListener('pageshow', this.nudge)
    document.removeEventListener('visibilitychange', this.nudge)
    this.clear()
    this.ws?.close(1000)
    this.ws = null
    this.o.onStatus?.('closed')
  }

  private clear() {
    this.timers.forEach(clearTimeout)
    this.timers = []
    if (this.ping) clearInterval(this.ping)
    this.ping = null
  }

  private async connect() {
    if (this.stopped) return
    this.o.onStatus?.('connecting')
    let ticket: string
    this.pending = true
    try {
      ticket = await this.o.getTicket()
    } catch (e: any) {
      if (e?.status === 401) { this.o.onSessionExpired?.(); this.stop(); return }
      if (!this.lostAt) this.lostAt = Date.now()
      return this.retry()
    } finally {
      this.pending = false
    }
    if (this.stopped) return
    const WS = this.o.WebSocketImpl ?? WebSocket
    const ws = new WS(`${this.o.url}${this.o.url.includes('?') ? '&' : '?'}ticket=${encodeURIComponent(ticket)}`)
    this.ws = ws
    ws.onopen = () => {
      debug.log('info', 'temps réel', `WebSocket ouvert${this.lostAt ? ` (${Date.now() - this.lostAt} ms après la coupure)` : ''}`)
      this.lostAt = 0
      this.lastSeen = Date.now()
      this.attempt = 0
      this.o.onStatus?.('open')
      if (this.everOpen) this.o.onReconnected?.()
      this.everOpen = true
      this.ping = setInterval(() => {
        if (Date.now() - this.lastSeen > DEAD_AFTER) { ws.close(); return }
        if (ws.readyState === 1) ws.send('{"type":"ping"}')
      }, PING_EVERY)
    }
    ws.onmessage = (m) => {
      this.lastSeen = Date.now()
      try {
        const ev = JSON.parse(m.data as string)
        if (ev.type !== 'pong') this.o.onEvent(ev)
      } catch { /* trame illisible : ignorée */ }
    }
    ws.onclose = (ev) => {
      if (this.ws !== ws) return
      debug.log('warn', 'temps réel', `WebSocket fermé (code ${ev.code})`)
      this.ws = null
      this.clear()
      if (!this.lostAt) this.lostAt = Date.now()
      this.o.onStatus?.('closed')
      if (this.stopped) return
      if (ev.code === 4401) { this.stopped = true; this.o.onSessionExpired?.(); return }
      if (ev.code === 4403) { this.stopped = true; this.o.onAccessRevoked?.(); return }
      this.retry()
    }
    ws.onerror = () => { /* onclose suit toujours */ }
  }

  private retry() {
    if (this.stopped) return
    const base = this.attempt++ === 0 ? 0 : Math.min(MAX_WAIT, 1000 * 2 ** (this.attempt - 2))
    const delay = base / 2 + Math.random() * (base / 2)
    this.timers.push(setTimeout(() => void this.connect(), delay))
  }
}

/** URL ws(s) de /ws d'après l'URL de l'API (ou l'origine courante si vide). */
export function wsUrl(apiUrl: string, origin = location.origin): string {
  const base = apiUrl || origin
  return base.replace(/^http/, 'ws').replace(/\/$/, '') + '/ws'
}
