import { api, ApiError, API_URL, setUnauthorizedHandler } from './api'
import { Realtime, wsUrl, type RealtimeEvent } from './realtime'
import { platform } from './platform'
import { prefs } from './prefs.svelte'
import { avatars, serverIcons } from '../blocks/avatars/avatars.svelte'
import { emojis } from '../blocks/emojis/emojis.svelte'
import { showNotification } from '../blocks/notifications/notify'
import { setupNotifications } from '../blocks/notifications/push'
import { lastViewHref } from './lastview'
import { DEFAULT_MAX_VIDEO } from './files'
import { parseHash, router } from './route.svelte'
import { voice } from '../blocks/vocal/voice.svelte'
import { blockOn } from '../blocks/registry'
import { videos } from '../blocks/videos/videos.svelte'
import { draw } from '../blocks/draw2gether/draw.svelte'
import { clockOffset, stale } from '../blocks/regarder-ensemble/watch'
import { notify } from './ui'
import { adminSections, rightsOf, type Right } from './admin'
import { convKeyOf, channelKey, dmKey, defaultNotifySettings } from './types'
import type { Me, Status, NotifySettings, PermKey, Channel, ConvKey, DM, Mention, Message, NewPoll, Read, Server, ServerDetail, VoiceParticipant, WatchState, WatchView, SessionKind } from './types'

export type Phase = 'loading' | 'login' | 'denied' | 'admin' | 'activate' | 'ready' | 'error'

export interface Conv {
  messages: Message[]
  hasMore: boolean
  loading: boolean
  loaded: boolean
}

const PAGE = 50
const MAX_JUMP = 5000 // messages rechargés au plus pour atteindre un message ancien
/** Les ids de message sont des entiers 64 bits en texte : on compare la longueur puis les caractères. */
const idAfter = (a: string, b: string) => a.length > b.length || (a.length === b.length && a > b)
const emptyConv = (): Conv => ({ messages: [], hasMore: false, loading: false, loaded: false })

/** État de l'application. Tout ce que le serveur pousse (WebSocket) passe par handleEvent. */
export class AppStore {
  phase = $state<Phase>('loading')
  error = $state('')
  meId = $state('')
  /** Taille maximale d'une vidéo en octets, annoncée par le serveur (`GET /me`). */
  maxVideo = $state(DEFAULT_MAX_VIDEO)
  /** Transcription des messages vocaux (EST-158) : désactivée, alpha (fonctions alpha débloquées) ou pour tous. */
  transcription = $state<'' | 'alpha' | 'on'>('')
  /** Blocs actifs annoncés par le serveur ; `undefined` (ancien serveur) : tous. */
  blocks = $state<string[] | undefined>(undefined)
  on = (name: string) => blockOn(this.blocks, name)
  pseudo = $state('')
  rights = $state<Right[]>([])
  isAdmin = $derived(this.rights.includes('admin'))
  adminSections = $derived(adminSections(this.rights, (b) => this.on(b)))
  movies = $state(false)
  servers = $state<Server[]>([])
  details = $state<Record<string, ServerDetail>>({})
  dms = $state<DM[]>([])
  convs = $state<Record<ConvKey, Conv>>({})
  online = $state<Record<string, true>>({})
  voice = $state<Record<string, VoiceParticipant[]>>({})
  /** Sessions « Regarder ensemble » par salon vocal, avec le décalage (ms) entre l'horloge du serveur et la nôtre. */
  watch = $state<Record<string, { state: WatchState; offset: number }>>({})
  /** Sessions Music2gether, séparées de `watch` (EST-168). */
  music = $state<Record<string, { state: WatchState; offset: number }>>({})
  cinema = $state<Record<string, { state: WatchState; offset: number }>>({})
  /** État des salons musique (file, morceau en cours), par id de salon. */
  /** Secondes passées en vocal par membre, par serveur. */
  voiceStats = $state<Record<string, Record<string, number>>>({})
  /** Heure (ms) du dernier relevé de `voiceStats`, par serveur. */
  voiceStatsAt = $state<Record<string, number>>({})
  mentions = $state<Mention[]>([])
  /** Conversations avec des messages arrivés pendant qu'elles n'étaient pas ouvertes (en mémoire seulement). */
  unread = $state<Record<ConvKey, number>>({})
  /** Statuts choisis (« idle », « dnd ») ; absent = en ligne. */
  statuses = $state<Record<string, Status>>({})
  /** Jeu en cours des autres (partagé par eux) ; le sien est dans `localActivity`, connu même sans le partager. */
  activities = $state<Record<string, string>>({})
  localActivity = $state('')
  notifySettings = $state<NotifySettings>(defaultNotifySettings())
  connection = $state<'connecting' | 'open' | 'closed'>('closed')
  /** Personnes en train d'écrire, par conversation (id → pseudo) ; entrées retirées au bout de 5 s. */
  typing = $state<Record<ConvKey, Record<string, string>>>({})
  /** Accusés de lecture : dernier message vu par personne (id → lecture), par conversation. */
  reads = $state<Record<ConvKey, Record<string, Read>>>({})
  /** Message auquel l'utilisateur est en train de répondre. */
  reply = $state<{ key: ConvKey; message: Message } | null>(null)
  /** Conversation actuellement affichée. */
  current = $state<ConvKey | null>(null)

  private rt: Realtime | null = null
  private readTimers = new Map<string, ReturnType<typeof setTimeout>>()
  private typingTimers = new Map<string, ReturnType<typeof setTimeout>>()

  constructor() {
    setUnauthorizedHandler(() => {
      if (this.phase === 'ready' || this.phase === 'activate') this.sessionLost()
      this.stopRealtime()
    })
    if (typeof window !== 'undefined') window.addEventListener('focus', () => this.current && this.markRead(this.current))
  }

  // --- démarrage ---

  async boot() {
    this.phase = 'loading'
    try {
      const me = await api.me()
      this.rights = rightsOf(me)
      if (!me.allowed) {
        this.phase = this.adminSections.length ? 'admin' : 'denied'
        return
      }
      if (!me.activated) {
        this.phase = 'activate'
        return
      }
      this.meId = me.id!
      this.pseudo = me.pseudo!
      this.applyMe(me)
      await this.loadAll()
      const fromNotification = platform.openFromNotification()
      if (fromNotification) router.current = parseHash(location.hash) // hashchange est asynchrone : sans cela, la dernière vue écraserait la cible
      // app mobile et PWA : on rouvre la dernière vue avant d'afficher quoi que ce soit (un lien ou une notification a priorité)
      const last = !fromNotification && platform.restoresLastView() && router.current.name === 'home' ? lastViewHref(this.meId, this.servers, this.dms) : null
      if (last) { history.replaceState(null, '', last); router.current = parseHash(last) }
      this.phase = 'ready'
      this.startRealtime()
      void platform.notifications.push?.refresh() // abonnement push à jour (jeton FCM, Web Push)
      void setupNotifications()
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) this.phase = 'login'
      else {
        this.error = e instanceof Error ? e.message : String(e)
        this.phase = 'error'
      }
    }
  }

  private applyMe(me: Me) {
    this.rights = rightsOf(me)
    if (!me.allowed || !me.activated) return
    this.maxVideo = me.max_video_size ?? DEFAULT_MAX_VIDEO
    this.transcription = me.transcription ?? ''
    this.blocks = me.blocks
    this.movies = me.movies === true && this.on('films')
  }

  async activate(pseudo: string) {
    await api.activate(pseudo.trim())
    await this.boot()
  }

  async loadAll() {
    const [servers, dms, mentions, unread] = await Promise.all([api.servers(), api.dms(), api.notifications(), api.unread()])
    this.servers = servers
    this.dms = dms
    for (const d of dms) if (d.online) this.online[d.other.id] = true
    this.mentions = mentions
    // Non lus conservés côté serveur ; la conversation affichée est considérée comme lue.
    this.unread = Object.fromEntries([...unread.channels.map((id) => [channelKey(id), unread.counts[id] ?? 1] as const), ...unread.dms.map((id) => [dmKey(id), unread.counts[id] ?? 1] as const)].filter(([k]) => k !== this.current))
    void avatars.load()
    api.statuses().then((s) => (this.statuses = s)).catch(() => {})
    void this.loadNotifySettings()
    void emojis.load()
    await Promise.all(servers.map((s) => this.loadServer(s.id)))
  }

  async loadServer(id: string) {
    const d = await api.server(id)
    this.details[id] = d
    for (const m of d.members) if (m.online) this.online[m.id] = true
    this.refreshVoice(id).catch(() => {})
    void avatars.loadServer(id)
    void this.refreshVoiceStats(id)
  }

  async refreshVoiceStats(serverId: string) {
    try {
      const stats = await api.voiceStats(serverId)
      this.voiceStats[serverId] = stats
      this.voiceStatsAt[serverId] = Date.now()
    } catch { /* affichage facultatif */ }
  }

  async refreshVoice(serverId: string) {
    try {
      const v = await api.serverVoice(serverId)
      for (const [cid, ps] of Object.entries(v)) this.voice[cid] = ps
    } catch { /* vocal indisponible : la liste reste vide */ }
    const kinds: SessionKind[] = this.on('films') ? ['watch', 'music', 'cinema'] : ['watch', 'music']
    await Promise.all(kinds.map(async (kind) => {
      try {
        const w = await api.serverWatch(serverId, kind)
        for (const [cid, v] of Object.entries(w)) this.setWatch(cid, v, true, kind)
      } catch { /* sessions inconnues : l'indicateur apparaîtra au prochain évènement */ }
    }))
  }

  setWatch(channelId: string, v: WatchView, fresh = false, kind: SessionKind = 'watch') {
    const sessions = this[kind]
    if (v.state && !fresh && stale(sessions[channelId]?.state, v.state)) return
    if (v.state) sessions[channelId] = { state: v.state, offset: clockOffset(v.now) }
    else delete sessions[channelId]
  }

  // --- temps réel ---

  startRealtime() {
    this.stopRealtime()
    this.rt = new Realtime({
      getTicket: async () => (await api.wsTicket()).ticket,
      url: `${wsUrl(API_URL)}?platform=${platform.id()}&v=${encodeURIComponent(__APP_VERSION__)}`,
      onEvent: (e) => this.handleEvent(e),
      onStatus: (s) => (this.connection = s),
      onReconnected: () => void this.resync(),
      onSessionExpired: () => void this.recheckSession(),
      onAccessRevoked: () => { this.phase = 'denied'; this.stopRealtime(); void voice.leave() },
    })
    this.rt.start()
  }

  /**
   * Le serveur ferme le WebSocket (4401) quand le justificatif de la connexion n'est plus valide : un Bearer dure
   * quelques minutes, un cookie peut aussi avoir été renouvelé. On revérifie avec le justificatif courant : si la
   * session est toujours bonne on se reconnecte, sinon retour à la connexion.
   */
  private async recheckSession() {
    try {
      this.applyMe(await api.me())
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) this.sessionLost()
      return
    }
    this.startRealtime()
    void this.resync()
  }

  /** Session perdue pour de bon : écran de connexion, et on quitte le vocal (sa connexion LiveKit survivrait au jeton). */
  private sessionLost() {
    this.phase = 'login'
    void voice.leave()
  }

  stopRealtime() {
    this.rt?.stop()
    this.rt = null
  }

  /** Après une coupure, des évènements ont pu être manqués : on recharge l'état et les conversations ouvertes. */
  async resync() {
    try {
      const [me] = await Promise.all([api.me().catch(() => null), this.loadAll()])
      if (me) this.applyMe(me)
      for (const [key, c] of Object.entries(this.convs)) if (c.loaded) await this.fetchFirstPage(key)
      for (const cid of Object.keys(videos.boards)) void videos.load(cid)
      draw.reloadAll()
    } catch { /* la prochaine reconnexion réessaiera */ }
  }

  handleEvent(ev: RealtimeEvent) {
    const d = ev.data
    switch (ev.type) {
      case 'presence.snapshot':
        this.online = Object.fromEntries((d.online as string[]).map((id) => [id, true]))
        this.activities = d.activities ?? {}
        break
      case 'avatar.updated':
        avatars.update(d.user_id as string, d.version as number)
        break
      case 'server_avatar.updated':
        avatars.updateServer(d.server_id as string, d.user_id as string, d.version as number)
        break
      case 'typing':
        this.onTyping(d.channel_id ? channelKey(d.channel_id) : dmKey(d.dm_id), d.user_id, d.pseudo)
        break
      case 'read.updated':
      {
        const key = d.channel_id ? channelKey(d.channel_id) : dmKey(d.dm_id)
        ;(this.reads[key] ??= {})[d.user_id] = d
        if (d.user_id === this.meId) this.clearLocalUnread(key)
        break
      }
      case 'status.updated':
        if (d.status === 'online') delete this.statuses[d.user_id]
        else this.statuses[d.user_id] = d.status
        break
      case 'activity.updated':
        if (d.name) this.activities[d.user_id] = d.name
        else delete this.activities[d.user_id]
        break
      case 'notify_settings.updated':
        this.notifySettings = d
        break
      case 'notification': // l'activité vocale de son propre salon s'affiche déjà sous les yeux
        if (!(d.kind === 'voice' && d.conv === channelKey(voice.channelId ?? ''))) showNotification(d, d.conv === this.current, this.notifIcon(d.url))
        break
      case 'emoji.updated':
        void emojis.load()
        break
      case 'presence':
        if (d.online) this.online[d.user_id] = true
        else { delete this.online[d.user_id]; delete this.activities[d.user_id] }
        break
      case 'message.created':
        this.onMessageCreated(d as Message)
        if (d.channel_id && d.reply_to) videos.changed(d.channel_id) // le nombre de réponses d'une carte
        break
      case 'message.updated':
        this.onMessageUpdated(d as Message)
        break
      case 'message.deleted':
        this.removeMessage(d.channel_id ? channelKey(d.channel_id) : dmKey(d.dm_id), d.id)
        if (d.channel_id) videos.changed(d.channel_id)
        break
      case 'reaction.updated':
        this.onReaction(d)
        videos.reaction(d.message_id, d.emoji, d.added, d.user_id === this.meId)
        break
      case 'poll.voted':
        this.onVote(d)
        break
      case 'poll.closed': {
        const m = this.findMessage(d.message_id)
        if (m?.poll) m.poll.closes_at = d.closes_at
        break
      }
      case 'mention':
        this.onMention(d as Message)
        break
      case 'channel.created': {
        const det = this.details[d.server_id]
        const c = d.channel as Channel
        if (!det || det.channels.some((x) => x.id === c.id)) break
        const ref = d.after ? det.channels.find((x) => x.id === d.after && x.kind === c.kind) : undefined
        if (ref) c.position = ref.position + 0.5
        det.channels.push(c)
        det.channels.sort((a, b) => (a.kind === b.kind ? a.position - b.position : a.kind < b.kind ? 1 : -1))
        break
      }
      case 'channel.updated': {
        const c = this.details[d.server_id]?.channels.find((c) => c.id === d.id)
        if (c) c.name = d.name
        break
      }
      case 'channel.reordered': {
        const det = this.details[d.server_id]
        if (!det) break
        ;(d.ids as string[]).forEach((id, i) => { const c = det.channels.find((c) => c.id === id); if (c) c.position = i + 1 })
        det.channels.sort((a, b) => (a.kind === b.kind ? a.position - b.position : a.kind < b.kind ? 1 : -1))
        break
      }
      case 'banner.updated':
        for (const det of Object.values(this.details)) { const m = det.members.find((m) => m.id === d.user_id); if (m) m.banner_color = d.color }
        break
      case 'channel.deleted': {
        const det = this.details[d.server_id]
        if (det) det.channels = det.channels.filter((c) => c.id !== d.id)
        delete this.convs[channelKey(d.id)]
        delete this.voice[d.id]
        delete this.watch[d.id]
        delete this.music[d.id]
        delete this.cinema[d.id]
        draw.drop(d.id)
        videos.drop(d.id)
        break
      }
      case 'server.updated': {
        const s = this.servers.find((s) => s.id === d.id)
        if (s) s.name = d.name
        if (this.details[d.id]) this.details[d.id].server.name = d.name
        break
      }
      case 'server_icon.updated': {
        const s = this.servers.find((s) => s.id === d.id)
        if (s) s.icon_version = d.version
        if (this.details[d.id]) this.details[d.id].server.icon_version = d.version
        break
      }
      case 'server.deleted':
        this.dropServer(d.id)
        break
      case 'member.joined': {
        const det = this.details[d.server_id]
        if (d.member.online) this.online[d.member.id] = true
        if (det && !det.members.some((m) => m.id === d.member.id)) det.members.push({ ...d.member, online: !!this.online[d.member.id] })
        else if (!det && d.member.id === this.meId) void this.refreshServers()
        break
      }
      case 'member.left':
        if (d.user_id === this.meId) {
          if (d.kind) notify.fail(`Vous avez été ${d.kind === 'ban' ? 'banni' : 'expulsé'} de « ${this.servers.find((s) => s.id === d.server_id)?.name ?? 'ce serveur'} »${d.reason ? ` : ${d.reason}` : ''}`)
          this.dropServer(d.server_id)
        } else if (this.details[d.server_id]) this.details[d.server_id].members = this.details[d.server_id].members.filter((m) => m.id !== d.user_id)
        break
      case 'roles.updated':
        if (this.details[d.server_id]) void this.reloadServer(d.server_id)
        break
      case 'member.timeout': {
        const m = this.details[d.server_id]?.members.find((m) => m.id === d.user_id)
        if (m) m.timeout_until = d.until
        if (d.user_id === this.meId) {
          void this.reloadServer(d.server_id)
          if (d.until) notify.fail(`Vous êtes exclu temporairement${d.reason ? ` : ${d.reason}` : ''}`)
          else notify.info('Votre exclusion temporaire est levée')
        }
        break
      }
      case 'member.warned':
        notify.fail(`Avertissement de la modération${d.reason ? ` : ${d.reason}` : ''}`)
        break
      case 'member.nickname': {
        const m = this.details[d.server_id]?.members.find((m) => m.id === d.user_id)
        if (m) m.nickname = d.nickname
        break
      }
      case 'messages.deleted':
        for (const id of d.ids) this.removeMessage(channelKey(d.channel_id), id)
        break
      case 'voice.moved':
        if (voice.channelId === d.from || (voice.channelId === null && voice.dropped?.id === d.from && Date.now() - (voice.dropped?.at ?? 0) < 5000)) {
          notify.info(`${d.by} vous a déplacé dans ${d.to_name}`)
          void voice.move(d.to)
        }
        break
      case 'voice.disconnected':
        notify.info('Vous avez été déconnecté du vocal par la modération')
        break
      case 'voice.state': {
        const list = (this.voice[d.channel_id] ?? []).filter((p) => p.user_id !== d.user_id)
        if (d.joined) list.push({ user_id: d.user_id, pseudo: d.pseudo, since: d.at })
        else void this.refreshVoiceStats(d.server_id)
        this.voice[d.channel_id] = list
        break
      }
      case 'watch.state':
        this.setWatch(d.channel_id, d)
        break
      case 'music.state':
        this.setWatch(d.channel_id, d, false, 'music')
        break
      case 'cinema.state':
        this.setWatch(d.channel_id, d, false, 'cinema')
        break
      case 'draw.state':
        draw.setState(d.channel_id, d)
        break
      case 'draw.private':
        draw.setPrivate(d.channel_id, d)
        break
      case 'draw.chat':
        draw.addChat(d.channel_id, d)
        break
      case 'draw.ops':
        draw.applyOps(d.channel_id, d, this.meId)
        break
      case 'videos.changed':
        videos.changed(d.channel_id)
        break
      case 'voice.share': {
        const p = this.voice[d.channel_id]?.find((p) => p.user_id === d.user_id)
        if (p) p[d.source as 'screen' | 'camera'] = d.on
        break
      }
      case 'voice.mod': {
        const p = this.voice[d.channel_id]?.find((p) => p.user_id === d.user_id)
        if (p) Object.assign(p, { mic_off: d.mic_off, sound_off: d.sound_off })
        break
      }
    }
  }

  /** Recharge le détail d'un serveur (rôles, permissions, membres) et programme le rechargement à la fin de ma propre exclusion. */
  async reloadServer(id: string) {
    try {
      const det = await api.server(id)
      if (!this.details[id]) return
      this.details[id] = det
      const s = this.servers.find((s) => s.id === id)
      if (s) s.owner_id = det.server.owner_id
      clearTimeout(this.timeoutTimers[id])
      const until = det.members.find((m) => m.id === this.meId)?.timeout_until
      if (until && Date.parse(until) > Date.now()) this.timeoutTimers[id] = setTimeout(() => void this.reloadServer(id), Date.parse(until) - Date.now() + 500)
    } catch { /* rechargé au prochain évènement */ }
  }
  private timeoutTimers: Record<string, ReturnType<typeof setTimeout>> = {}

  async refreshServers() {
    this.servers = await api.servers()
    await Promise.all(this.servers.filter((s) => !this.details[s.id]).map((s) => this.loadServer(s.id)))
  }

  dropServer(id: string) {
    this.servers = this.servers.filter((s) => s.id !== id)
    for (const c of this.details[id]?.channels ?? []) {
      delete this.convs[channelKey(c.id)]
      delete this.unread[channelKey(c.id)]
      delete this.voice[c.id]
    }
    delete this.details[id]
  }

  // --- messages ---

  conv(key: ConvKey): Conv {
    return this.convs[key] ?? emptyConv()
  }

  private async fetchFirstPage(key: ConvKey) {
    const page = key.startsWith('c:') ? await api.channelMessages(key.slice(2), undefined, PAGE) : await api.dmMessages(key.slice(2), undefined, PAGE)
    this.convs[key] = { messages: page.messages, hasMore: page.has_more, loading: false, loaded: true }
    void api.reads(key).then((l) => (this.reads[key] = Object.fromEntries(l.map((r) => [r.user_id, r]))), () => {})
  }

  /** Icône (déjà chargée) du serveur visé par l'adresse d'une notification (« /#/s/<serveur>/… »). */
  private notifIcon(url: string): string {
    const s = this.servers.find((s) => url.startsWith(`/#/s/${s.id}`))
    return s ? serverIcons.src(s.id, s.icon_version) : ''
  }

  /** Avatars à afficher sous chaque message : les autres membres, sous le dernier message chargé qu'ils ont vu. */
  readers(key: ConvKey): Record<string, Read[]> {
    const out: Record<string, Read[]> = {}
    const msgs = this.convs[key]?.messages ?? []
    for (const r of Object.values(this.reads[key] ?? {})) {
      if (r.user_id === this.meId) continue
      const m = msgs.filter((x) => !idAfter(x.id, r.last_id)).at(-1)
      if (m) (out[m.id] ??= []).push(r)
    }
    for (const l of Object.values(out)) l.sort((a, b) => a.read_at.localeCompare(b.read_at))
    return out
  }

  /** Affiche une conversation : charge la première page, efface la pastille et les mentions lues. */
  async open(key: ConvKey | null) {
    this.current = key
    if (!key) return
    delete this.unread[key]
    this.markMentionsRead(key)
    if (document.hasFocus()) this.markRead(key)
    if (this.convs[key]?.loaded || this.convs[key]?.loading) return
    this.convs[key] = { ...emptyConv(), loading: true }
    try {
      await this.fetchFirstPage(key)
    } catch (e) {
      delete this.convs[key]
      throw e
    }
  }

  async loadOlder(key: ConvKey) {
    const c = this.convs[key]
    if (!c || c.loading || !c.hasMore || c.messages.length === 0) return
    c.loading = true
    try {
      const before = c.messages[0].id
      const page = key.startsWith('c:') ? await api.channelMessages(key.slice(2), before, PAGE) : await api.dmMessages(key.slice(2), before, PAGE)
      const known = new Set(c.messages.map((m) => m.id))
      c.messages = [...page.messages.filter((m) => !known.has(m.id)), ...c.messages]
      c.hasMore = page.has_more
    } finally {
      c.loading = false
    }
  }

  /**
   * Message trop ancien pour la pagination vers le haut : remplace l'historique chargé par les messages depuis `id` jusqu'au présent
   * (pas de trou, les nouveaux messages continuent de s'ajouter). Faux si le message n'existe plus ou si le présent est à plus de MAX_JUMP messages.
   */
  async loadFrom(key: ConvKey, id: string): Promise<boolean> {
    const c = this.convs[key]
    if (!c) return false
    const fetchAfter = (after: string) => key.startsWith('c:') ? api.channelMessages(key.slice(2), undefined, 100, after) : api.dmMessages(key.slice(2), undefined, 100, after)
    const msgs: Message[] = []
    let after = (BigInt(id) - 1n).toString()
    for (let more = true; more; ) {
      const page = await fetchAfter(after)
      if (msgs.length === 0 && page.messages[0]?.id !== id) return false
      msgs.push(...page.messages)
      more = page.has_more
      if (more && msgs.length >= MAX_JUMP) return false
      after = page.messages.at(-1)?.id ?? after
    }
    const last = msgs.at(-1)!.id
    c.messages = [...msgs, ...c.messages.filter((m) => idAfter(m.id, last))]
    c.hasMore = true
    return true
  }

  async send(key: ConvKey, content: string, fileIds: string[] = [], replyTo?: string, poll?: NewPoll) {
    const m = key.startsWith('c:') ? await api.sendChannelMessage(key.slice(2), content, fileIds, replyTo, poll) : await api.sendDMMessage(key.slice(2), content, fileIds, replyTo, poll)
    this.appendMessage(key, m) // sans attendre l'évènement du WebSocket (dédoublonné ensuite)
    return m
  }

  private appendMessage(key: ConvKey, m: Message) {
    const c = this.convs[key]
    if (c?.loaded && !c.messages.some((x) => x.id === m.id)) c.messages.push(m)
  }

  private onMessageCreated(m: Message) {
    const key = convKeyOf(m)
    if (key.startsWith('d:') && !this.dms.some((d) => d.id === m.dm_id)) void api.dms().then((l) => { this.dms = l; for (const d of l) if (d.online) this.online[d.other.id] = true })
    this.appendMessage(key, m)
    const mine = m.author.id === this.meId
    this.stopTyping(key, m.author.id)
    if (!mine && this.current !== key) this.unread[key] = (this.unread[key] ?? 0) + 1
    else if (!mine && document.hasFocus()) this.markRead(key)
  }

  private onTyping(key: ConvKey, userId: string, pseudo: string) {
    if (userId === this.meId) return
    this.typing[key] = { ...this.typing[key], [userId]: pseudo }
    const id = key + userId
    clearTimeout(this.typingTimers.get(id))
    this.typingTimers.set(id, setTimeout(() => this.stopTyping(key, userId), 5000))
  }

  private stopTyping(key: ConvKey, userId: string) {
    clearTimeout(this.typingTimers.get(key + userId))
    this.typingTimers.delete(key + userId)
    const t = this.typing[key]
    if (t && userId in t) delete t[userId]
  }

  /** Pseudos des personnes qui écrivent dans la conversation. */
  typingNames(key: ConvKey): string[] {
    return Object.values(this.typing[key] ?? {})
  }

  private onMessageUpdated(m: Message) {
    const c = this.convs[convKeyOf(m)]
    const i = c?.messages.findIndex((x) => x.id === m.id) ?? -1
    if (!c || i < 0) return
    // Les réactions de l'évènement sont calculées pour l'auteur de la modification : on garde les nôtres.
    c.messages[i] = { ...m, reactions: c.messages[i].reactions }
  }

  /** Supprime le message côté serveur puis l'ôte de l'affichage sans attendre l'évènement temps réel. */
  async deleteMessage(key: ConvKey, id: string) {
    await api.deleteMessage(id)
    this.removeMessage(key, id)
  }

  private removeMessage(key: ConvKey, id: string) {
    const c = this.convs[key]
    if (c) c.messages = c.messages.filter((m) => m.id !== id)
    this.mentions = this.mentions.filter((x) => x.message_id !== id)
  }

  private onReaction(d: { message_id: string; emoji: string; user_id: string; pseudo?: string; added: boolean }) {
    for (const c of Object.values(this.convs)) {
      const m = c.messages.find((x) => x.id === d.message_id)
      if (!m) continue
      const r = m.reactions.find((x) => x.emoji === d.emoji)
      const mine = d.user_id === this.meId
      if (d.added) {
        if (r && mine && r.me) return // déjà pris en compte localement
        const who = { id: d.user_id, pseudo: d.pseudo ?? '' }
        if (r) { r.count++; if (mine) r.me = true; if (!r.users.some((x) => x.id === who.id)) r.users.push(who) }
        else m.reactions.push({ emoji: d.emoji, count: 1, me: mine, users: [who] })
      } else if (r) {
        if (mine && !r.me) return
        r.count--
        if (mine) r.me = false
        r.users = r.users.filter((x) => x.id !== d.user_id)
        if (r.count <= 0) m.reactions = m.reactions.filter((x) => x.emoji !== d.emoji)
      }
      return
    }
  }

  private findMessage(id: string) {
    for (const c of Object.values(this.convs)) {
      const m = c.messages.find((x) => x.id === id)
      if (m) return m
    }
  }

  private onVote(d: { message_id: string; user_id: string; options: string[]; counts: Record<string, number> }) {
    for (const c of Object.values(this.convs)) {
      const m = c.messages.find((x) => x.id === d.message_id)
      if (!m?.poll) continue
      for (const o of m.poll.options) {
        o.votes = d.counts[o.id] ?? 0
        if (d.user_id === this.meId) o.me = d.options.includes(o.id)
      }
      return
    }
  }

  /** Vote (recliquer sur son choix le retire) ; les totaux viennent de l'évènement. */
  async vote(m: Message, optionId: string) {
    if (m.poll?.options.find((o) => o.id === optionId)?.me) await api.unvote(m.id, optionId)
    else await api.vote(m.id, optionId)
  }

  /** Ajoute ou retire sa réaction (mise à jour immédiate, annulée en cas d'erreur). */
  async toggleReaction(m: Message, emoji: string) {
    const r = m.reactions.find((x) => x.emoji === emoji)
    const had = !!r?.me
    this.onReaction({ message_id: m.id, emoji, user_id: this.meId, pseudo: this.pseudo, added: !had })
    try {
      if (had) await api.unreact(m.id, emoji)
      else await api.react(m.id, emoji)
    } catch (e) {
      this.onReaction({ message_id: m.id, emoji, user_id: this.meId, pseudo: this.pseudo, added: had })
      throw e
    }
  }

  // --- mentions ---

  private onMention(m: Message) {
    if (this.mentions.some((x) => x.message_id === m.id)) return
    const d = m.channel_id ? Object.values(this.details).find((s) => s.channels.some((c) => c.id === m.channel_id)) : undefined
    this.mentions.unshift({
      message_id: m.id, channel_id: m.channel_id, server_id: d?.server.id, dm_id: m.dm_id,
      author: m.author, content: m.content, created_at: m.created_at, group: (m as Message & { group_mention?: boolean }).group_mention,
    })
    if (this.current && convKeyOf(m) === this.current) this.markMentionsRead(this.current)
  }

  private get counted() {
    const ignored = this.notifySettings.ignore_group ?? []
    return this.mentions.filter((m) => !(m.group && m.server_id && ignored.includes(m.server_id)))
  }

  mentionCount(key: ConvKey): number {
    return this.counted.filter((m) => convKeyOf(m) === key).length
  }

  serverMentionCount(serverId: string): number {
    return this.counted.filter((m) => m.server_id === serverId).length
  }

  /** Marque une conversation lue à la demande de l'utilisateur (action d'une notification). */
  readConv(key: ConvKey) {
    delete this.unread[key]
    this.markMentionsRead(key)
    this.markRead(key)
  }

  /** Mémorise côté serveur que la conversation est lue (échec ignoré : la pastille reviendra au prochain chargement). */
  private markRead(key: ConvKey) {
    platform.cancelNotification(key)
    clearTimeout(this.readTimers.get(key))
    this.readTimers.set(key, setTimeout(() => api.markRead(key).catch(() => {}), 300)) // regroupe les rafales de messages
  }

  /** Lecture faite depuis un autre appareil : efface pastille et mentions sans rappeler le serveur. */
  private clearLocalUnread(key: ConvKey) {
    delete this.unread[key]
    this.mentions = this.mentions.filter((m) => convKeyOf(m) !== key)
  }

  private markMentionsRead(key: ConvKey) {
    const ids = this.mentions.filter((m) => convKeyOf(m) === key).map((m) => m.message_id)
    if (!ids.length) return
    this.mentions = this.mentions.filter((m) => !ids.includes(m.message_id))
    api.readNotifications(ids).catch(() => {})
  }

  // --- aides ---

  channel(channelId: string): { server: Server; channel: Channel } | null {
    for (const d of Object.values(this.details)) {
      const channel = d.channels.find((c) => c.id === channelId)
      if (channel) return { server: d.server, channel }
    }
    return null
  }

  /** Pseudos connus d'une conversation, pour les mentions. */
  /** Heure d'arrivée (ms) de l'utilisateur dans un salon vocal du serveur, null s'il n'y est pas (ou heure inconnue). */
  voiceSinceOf(serverId: string, userId: string): number | null {
    for (const c of this.details[serverId]?.channels ?? []) {
      const t = Date.parse(this.voice[c.id]?.find((p) => p.user_id === userId)?.since ?? '')
      if (!Number.isNaN(t)) return t
    }
    return null
  }

  /** Présence à afficher : hors ligne, en ligne, absent ou ne pas déranger. */
  presenceOf(id: string): 'off' | Status {
    // on est forcément connecté soi-même, même si le serveur ne nous a pas (encore) listé en ligne
    return id === this.meId || this.online[id] ? (this.statuses[id] ?? 'online') : 'off'
  }

  /** Jeu en cours à afficher pour un utilisateur ('' si aucun ou hors ligne). */
  /** Morceau écouté par une personne présente dans un salon vocal où une session Music2gether est en cours (« 🎵 Écoute … »). */
  listeningOf(id: string): string {
    for (const [cid, ps] of Object.entries(this.voice)) {
      const m = this.music[cid]?.state.meta
      if (m && ps.some((p) => p.user_id === id)) return [m.title, m.artist].filter(Boolean).join(' – ')
    }
    return ''
  }
  activityOf(id: string): string {
    return id === this.meId ? this.localActivity : this.online[id] ? (this.activities[id] ?? '') : ''
  }

  async setStatus(s: Status) {
    const before = this.statuses[this.meId]
    if (s === 'online') delete this.statuses[this.meId]
    else this.statuses[this.meId] = s
    try {
      await api.setStatus(s)
    } catch (e) {
      if (before) this.statuses[this.meId] = before
      else delete this.statuses[this.meId]
      throw e
    }
  }

  async loadNotifySettings() {
    try {
      this.notifySettings = await api.notifySettings()
    } catch { /* réglages par défaut */ }
  }

  async saveNotifySettings(patch: Partial<NotifySettings>) {
    const before = this.notifySettings
    this.notifySettings = { ...before, ...patch }
    try {
      await api.setNotifySettings($state.snapshot(this.notifySettings))
    } catch (e) {
      this.notifySettings = before
      throw e
    }
  }

  /** Permission de l'utilisateur dans un salon (ou au niveau du serveur sans salon). Vraie en message privé. */
  can(serverId: string | undefined, perm: PermKey, channelId?: string): boolean {
    const d = serverId ? this.details[serverId] : undefined
    if (!d) return true // message privé, ou serveur pas encore chargé : le serveur revérifie de toute façon
    const channel = channelId ? d.channels.find((c) => c.id === channelId) : undefined
    return (channel?.permissions ?? d.permissions)?.includes(perm) ?? true
  }

  /** Permission dans la conversation affichée (toujours vraie en message privé). */
  canConv(key: ConvKey | null, perm: PermKey): boolean {
    if (!key?.startsWith('c:')) return true
    const info = this.channel(key.slice(2))
    return info ? this.can(info.server.id, perm, info.channel.id) : true
  }

  /** Noms de rôle et @everyone, pour mettre en évidence leurs mentions dans un message. */
  groupMentionsOf(key: ConvKey | null): string[] {
    const info = key?.startsWith('c:') ? this.channel(key.slice(2)) : null
    return info ? ['everyone', 'here', ...(this.details[info.server.id].roles ?? []).filter((r) => !r.is_default).map((r) => r.name)] : []
  }

  /** Mentions de groupe proposables à la saisie : seulement si « Mentionner @everyone et les rôles » est permis. */
  groupSuggestionsOf(key: ConvKey | null): string[] {
    return this.canConv(key, 'mention_everyone') ? this.groupMentionsOf(key) : []
  }

  /** Personnes touchées par @everyone ou @here (membres du serveur, ou membres en ligne), auteur exclu. */
  groupCountOf(key: ConvKey | null, name: string): number | null {
    const info = key?.startsWith('c:') ? this.channel(key.slice(2)) : null
    if (!info || (name !== 'everyone' && name !== 'here')) return null
    const members = this.details[info.server.id].members.filter((m) => m.id !== this.meId)
    return name === 'everyone' ? members.length : members.filter((m) => m.online).length
  }

  pseudosOf(key: ConvKey | null): string[] {
    if (!key) return []
    if (key.startsWith('d:')) {
      const dm = this.dms.find((d) => d.id === key.slice(2))
      return dm ? [this.pseudo, dm.other.pseudo] : [this.pseudo]
    }
    const info = this.channel(key.slice(2))
    return info ? this.details[info.server.id].members.map((m) => m.pseudo) : []
  }

  /** Total des mentions non lues, pour l'indicateur de l'onglet / de l'icône. */
  get totalMentions() {
    return this.counted.length
  }

  prefsChanged() {
    prefs.save()
  }
}

export const app = new AppStore()
