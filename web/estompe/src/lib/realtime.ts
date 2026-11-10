import { debug } from './debuglog.svelte'
import type { ChatLine, DrawOp, DrawPrivate, DrawState } from '../blocks/draw2gether/draw'
import type { Channel, Member, Message, NotifySettings, Read, ServerNotification, Status, WatchView } from './types'

type Conversation = { channel_id: string; dm_id?: undefined } | { channel_id?: undefined; dm_id: string }
type Salon = { channel_id: string }

export interface RealtimeEvents {
  'presence.snapshot': { online: string[]; activities?: Record<string, string> }
  'avatar.updated': { user_id: string; version: number }
  'server_avatar.updated': { server_id: string; user_id: string; version: number }
  typing: Conversation & { user_id: string; pseudo: string }
  'read.updated': Conversation & Read
  'status.updated': { user_id: string; status: Status }
  'activity.updated': { user_id: string; name?: string | null }
  'notify_settings.updated': NotifySettings
  notification: ServerNotification
  'emoji.updated': { server_id: string }
  presence: { user_id: string; online: boolean }
  'message.created': Message
  'message.updated': Message
  'message.deleted': Conversation & { id: string }
  'reaction.updated': { message_id: string; emoji: string; user_id: string; pseudo?: string; added: boolean }
  'poll.voted': { message_id: string; user_id: string; options: string[]; counts: Record<string, number> }
  'poll.closed': { message_id: string; closes_at: string }
  mention: Message
  'channel.created': { server_id: string; channel: Channel; after?: string }
  'channel.updated': { server_id: string; id: string; name: string }
  'channel.reordered': { server_id: string; ids: string[] }
  'banner.updated': { user_id: string; color?: string }
  'channel.deleted': { server_id: string; id: string }
  'server.updated': { id: string; name: string }
  'server_icon.updated': { id: string; version: number }
  'server.deleted': { id: string }
  'member.joined': { server_id: string; member: Member }
  'member.left': { server_id: string; user_id: string; kind?: 'kick' | 'ban' | null; reason?: string | null; until?: string | null }
  'roles.updated': { server_id: string }
  'member.timeout': { server_id: string; user_id: string; until: string | null; reason?: string }
  'member.warned': { server_id: string; reason?: string }
  'member.nickname': { server_id: string; user_id: string; nickname?: string }
  'messages.deleted': Salon & { ids: string[] }
  'voice.moved': { from: string; to: string; to_name: string; by: string }
  'voice.disconnected': Salon & { server_id: string }
  'voice.state': Salon & { server_id: string; user_id: string; pseudo: string; joined: boolean; at?: string }
  'watch.state': Salon & WatchView
  'music.state': Salon & WatchView
  'cinema.state': Salon & WatchView
  'draw.state': Salon & { state: DrawState | null; now: number }
  'draw.private': Salon & DrawPrivate & { rev: number }
  'draw.chat': Salon & ChatLine
  'draw.ops': Salon & { seq: number; ops: DrawOp[] }
  'videos.changed': Salon
  'voice.share': Salon & { user_id: string; source: 'screen' | 'camera'; on: boolean }
  'voice.mod': Salon & { user_id: string; mic_off: boolean; sound_off: boolean }
  pong: Record<string, never>
}

export type RealtimeEvent = { [K in keyof RealtimeEvents]: { type: K; data: RealtimeEvents[K] } }[keyof RealtimeEvents]
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
        const ev = JSON.parse(m.data as string) as RealtimeEvent
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
