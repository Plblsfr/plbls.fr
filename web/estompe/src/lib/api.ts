import { config } from './config'
import { debug } from './debuglog.svelte'
import type { DrawOp, DrawView } from '../blocks/draw2gether/draw'
import type { FilmDetail, FilmItem, FilmPlayback, FilmReject, PlaybackRequest } from '../blocks/films/films'
import type { EntracteMe, EntracteNotification, EntracteRequest, EntracteStatus, RequestKind, RequestableSeries, SearchResult } from '../sections/entracte/entracte'
import type {
  AdminLive, AdminOverview, AdminBadge, Badge, BadgeInput, AdminUser, AdminStats, AdminLogEntry, AdminError,
  Person,
  Attachment, Channel, DM, LinkPreview, Me, Mention, Message, MessagePage,
  Server, ServerDetail, ServerInvite, Role, RoleInput, Override, AuditEntry, Ban, PermKey, VoiceParticipant, VoiceToken, WatchView, SessionKind, MusicTrack, CustomEmoji, GifResult, Status, NotifySettings,
  NewPoll, Read, Report, ReportInput, ReportStatus, ChannelKind, NewChannel, VideosBoard, VideoList, WatchSession, Rsvp,
} from './types'

export const API_URL = config.apiUrl
export const ENTRACTE_URL = config.entracteUrl

export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message)
  }
}

type TokenProvider = () => Promise<string | null> | string | null
let tokenProvider: TokenProvider | null = null
let onUnauthorized: (() => void) | null = null
let onUpgradeRequired: ((message: string) => void) | null = null
let clientPlatform = ''

/** Application Windows : jeton d'accès obtenu par PKCE, envoyé en Bearer. Sur le web, le cookie de session suffit. */
export function setTokenProvider(p: TokenProvider | null) { tokenProvider = p }
/** Vrai en mode Bearer (app Windows) : les <img> ne peuvent pas envoyer l'en-tête, il faut télécharger le fichier. */
export const usesBearer = () => tokenProvider !== null
export function setUnauthorizedHandler(h: (() => void) | null) { onUnauthorized = h }
/** Appelé quand l'API refuse la version de l'app (426) : l'app affiche l'écran « Mise à jour requise ». */
export function setUpgradeHandler(h: ((message: string) => void) | null) { onUpgradeRequired = h }
/** Plateforme déclarée à l'API avec la version (en-têtes X-Estompe-Platform et X-Estompe-Version). */
export function setClientPlatform(p: string) { clientPlatform = p }

async function headers(json: boolean): Promise<Record<string, string>> {
  const h: Record<string, string> = {}
  if (clientPlatform) { h['X-Estompe-Platform'] = clientPlatform; h['X-Estompe-Version'] = __APP_VERSION__ }
  if (json) h['Content-Type'] = 'application/json'
  const t = tokenProvider ? await tokenProvider() : null
  if (t) h.Authorization = `Bearer ${t}`
  return h
}

async function requestAt<T>(base: string, method: string, path: string, body?: unknown): Promise<T> {
  const attempt = async () => {
    const init: RequestInit = {
      method,
      credentials: 'include',
      headers: await headers(body !== undefined && !(body instanceof FormData)),
    }
    if (body instanceof FormData) init.body = body
    else if (body !== undefined) init.body = JSON.stringify(body)
    return fetch(base + path, init)
  }
  let res = await attempt().catch((e) => { debug.log('error', 'réseau', `${method} ${path.split('?')[0]} ✗ ${e}`); throw e })
  // Un 401 isolé peut venir d'un jeton expiré en route ou d'un cookie renouvelé : on réessaie une fois avant de conclure.
  if (res.status === 401) res = await attempt()
  // 429 sur une lecture : une seule relance, après le délai du serveur (jamais pour une action de la personne, qui l'indique et la laisse réessayer)
  if (res.status === 429 && method === 'GET') {
    await new Promise((r) => setTimeout(r, Math.min(Math.max(Number(res.headers.get('Retry-After')) || 1, 1), 30) * 1000))
    res = await attempt()
  }
  debug.log(res.ok ? 'info' : 'warn', 'réseau', `${method} ${path.split('?')[0]} → ${res.status}`)
  if (!res.ok) {
    let code = 'http_' + res.status
    let msg = res.statusText
    try {
      const e = await res.json()
      code = e.error ?? code
      msg = e.message ?? msg
    } catch { /* corps non JSON */ }
    debug.log('warn', 'réseau', `${method} ${path.split('?')[0]} ✗ ${code} : ${msg}`)
    if (res.status === 401) onUnauthorized?.()
    if (res.status === 426 && base === API_URL) onUpgradeRequired?.(msg)
    throw new ApiError(res.status, code, msg)
  }
  if (res.status === 204) return undefined as T
  const ct = res.headers.get('Content-Type') ?? ''
  return (ct.includes('json') ? res.json() : undefined) as Promise<T>
}

const request = <T>(method: string, path: string, body?: unknown) => requestAt<T>(API_URL, method, path, body)

/** Envoie une image en corps brut (PUT), avec son type. */
async function putImage(path: string, img: Blob) {
  const res = await fetch(API_URL + path, { method: 'PUT', credentials: 'include', headers: { ...(await headers(false)), 'Content-Type': img.type }, body: img })
  if (!res.ok) {
    const e = await res.json().catch(() => ({}))
    throw new ApiError(res.status, e.error ?? 'http_' + res.status, e.message ?? res.statusText)
  }
}

/** Télécharge une image avec l'authentification et la rend en URL locale. */
async function imageAt(base: string, path: string) {
  const res = await fetch(base + path, { credentials: 'include', headers: await headers(false) })
  if (!res.ok) throw new ApiError(res.status, 'http_' + res.status, res.statusText)
  return URL.createObjectURL(await res.blob())
}
const imageUrl = (path: string) => imageAt(API_URL, path)

const enc = encodeURIComponent
/** Préfixe d'API d'une conversation d'après sa clé ("c:<id>" ou "d:<id>"). */
const convPath = (key: string) => (key.startsWith('c:') ? `/channels/${key.slice(2)}` : `/dms/${key.slice(2)}`)
const get = <T>(p: string) => request<T>('GET', p)
const post = <T>(p: string, b?: unknown) => request<T>('POST', p, b ?? {})
const send = <T>(m: string, p: string, b?: unknown) => request<T>(m, p, b)

export const api = {
  me: () => get<Me>('/me'),
  permissions: <T = void>(methode: string, chemin: string, corps?: unknown) => send<T>(methode, `/admin/permissions${chemin}`, corps),
  acces: <T = void>(methode: string, chemin: string, corps?: unknown) => send<T>(methode, `/admin/acces${chemin}`, corps),
  adminLive: () => get<AdminLive>('/admin/live'),
  adminOverview: () => get<AdminOverview>('/admin/overview'),
  adminBadges: () => get<AdminBadge[]>('/admin/badges'),
  createBadge: (b: BadgeInput) => post<Badge>('/admin/badges', b),
  deleteBadge: (id: string) => send<void>('DELETE', `/admin/badges/${id}`),
  grantBadge: (id: string, pseudo: string) => send<void>('PUT', `/admin/badges/${id}/users/${enc(pseudo)}`),
  revokeBadge: (id: string, pseudo: string) => send<void>('DELETE', `/admin/badges/${id}/users/${enc(pseudo)}`),
  adminUsers: (q: string) => get<AdminUser[]>(`/admin/users?q=${enc(q)}`),
  suspendUser: (id: string, on: boolean) => send<void>(on ? 'PUT' : 'DELETE', `/admin/users/${id}/suspension`),
  adminDeleteServer: (id: string) => send<void>('DELETE', `/admin/servers/${id}`),
  adminStats: () => get<AdminStats>('/admin/stats'),
  adminLog: () => get<AdminLogEntry[]>('/admin/log'),
  adminErrors: () => get<AdminError[]>('/admin/errors'),
  adminBot: () => get<{ pseudo: string; releases: string[] }>('/admin/bot'),
  botMessage: (content: string, to: string) => post<{ sent: number }>('/admin/bot/messages', { content, to }),
  adminReports: (f: Record<string, string>) => get<Report[]>(`/admin/reports?${new URLSearchParams(Object.entries(f).filter(([, v]) => v))}`),
  adminReport: (id: string) => get<Report>(`/admin/reports/${id}`),
  updateReport: (id: string, r: { status: string; duplicate_of?: string; notes: string }) => send<Report>('PATCH', `/admin/reports/${id}`, r),
  deleteReport: (id: string) => send<void>('DELETE', `/admin/reports/${id}`),
  sendReport: (r: ReportInput) => post<{ id: string; status: ReportStatus }>('/reports', r),
  myReports: () => get<Report[]>('/reports'),
  userBadges: (id: string) => get<Badge[]>(`/users/${id}/badges`),
  activate: (pseudo: string) => post<{ id: string; pseudo: string }>('/activate', { pseudo }),

  servers: () => get<Server[]>('/servers'),
  createServer: (name: string) => post<Server>('/servers', { name }),
  server: (id: string) => get<ServerDetail>(`/servers/${id}`),
  renameServer: (id: string, name: string) => send<void>('PATCH', `/servers/${id}`, { name }),
  deleteServer: (id: string) => send<void>('DELETE', `/servers/${id}`),
  leaveServer: (id: string) => post<void>(`/servers/${id}/leave`),
  createChannel: (sid: string, b: NewChannel) => post<Channel>(`/servers/${sid}/channels`, b),
  renameChannel: (sid: string, cid: string, name: string) => send<void>('PATCH', `/servers/${sid}/channels/${cid}`, { name }),
  reorderChannels: (sid: string, kind: ChannelKind, ids: string[]) => send<void>('PUT', `/servers/${sid}/channels/order`, { kind, ids }),
  deleteChannel: (sid: string, cid: string) => send<void>('DELETE', `/servers/${sid}/channels/${cid}`),
  serverInvites: (sid: string) => get<ServerInvite[]>(`/servers/${sid}/invites`),
  createServerInvite: (sid: string, opts: { max_uses?: number; expires_in?: number } = {}) => post<ServerInvite>(`/servers/${sid}/invites`, opts),
  revokeServerInvite: (sid: string, code: string) => send<void>('DELETE', `/servers/${sid}/invites/${enc(code)}`),
  createRole: (sid: string, r: RoleInput) => post<Role>(`/servers/${sid}/roles`, r),
  updateRole: (sid: string, rid: string, r: Partial<RoleInput>) => send<Role>('PATCH', `/servers/${sid}/roles/${rid}`, r),
  deleteRole: (sid: string, rid: string) => send<void>('DELETE', `/servers/${sid}/roles/${rid}`),
  reorderRoles: (sid: string, ids: string[]) => send<void>('PUT', `/servers/${sid}/roles-order`, { ids }),
  setMemberRole: (sid: string, uid: string, rid: string, on: boolean) => send<void>(on ? 'PUT' : 'DELETE', `/servers/${sid}/members/${uid}/roles/${rid}`),
  channelOverrides: (sid: string, cid: string) => get<Override[]>(`/servers/${sid}/channels/${cid}/overrides`),
  setOverride: (sid: string, cid: string, rid: string, allow: PermKey[], deny: PermKey[]) => send<void>('PUT', `/servers/${sid}/channels/${cid}/overrides/${rid}`, { allow, deny }),
  auditLog: (sid: string, f: { action?: string; target?: string } = {}) => get<AuditEntry[]>(`/servers/${sid}/audit-log?${new URLSearchParams(Object.entries(f).filter(([, v]) => v) as [string, string][])}`),
  kickMember: (sid: string, uid: string, reason: string) => post<void>(`/servers/${sid}/members/${uid}/kick`, { reason }),
  banMember: (sid: string, uid: string, b: { reason: string; duration: number; delete_messages: number }) => post<void>(`/servers/${sid}/members/${uid}/ban`, b),
  bans: (sid: string) => get<Ban[]>(`/servers/${sid}/bans`),
  unban: (sid: string, uid: string) => send<void>('DELETE', `/servers/${sid}/bans/${uid}`),
  timeoutMember: (sid: string, uid: string, duration: number, reason: string) => send<void>('PUT', `/servers/${sid}/members/${uid}/timeout`, { duration, reason }),
  untimeoutMember: (sid: string, uid: string) => send<void>('DELETE', `/servers/${sid}/members/${uid}/timeout`),
  warnMember: (sid: string, uid: string, reason: string) => post<void>(`/servers/${sid}/members/${uid}/warn`, { reason }),
  setNickname: (sid: string, uid: string, nickname: string) => send<void>('PUT', `/servers/${sid}/members/${uid}/nickname`, { nickname }),
  memberHistory: (sid: string, uid: string) => get<AuditEntry[]>(`/servers/${sid}/members/${uid}/history`),
  channelModeration: (sid: string, cid: string, slowmode: number, locked: boolean) => send<void>('PUT', `/servers/${sid}/channels/${cid}/moderation`, { slowmode, locked }),
  bulkDelete: (cid: string, b: { ids?: string[]; last?: number; author?: string }) => post<{ deleted: string[] }>(`/channels/${cid}/messages/bulk-delete`, b),
  voiceMove: (cid: string, uid: string, channel_id: string) => send<void>('PUT', `/voice/${cid}/participants/${uid}/move`, { channel_id }),
  voiceDisconnect: (cid: string, uid: string) => post<void>(`/voice/${cid}/participants/${uid}/disconnect`),
  transferServer: (sid: string, user_id: string) => post<void>(`/servers/${sid}/transfer`, { user_id }),
  joinServer: (code: string) => post<Server>(`/server-invites/${enc(code)}/join`),
  serverWatch: (sid: string, kind: SessionKind = 'watch') => get<Record<string, WatchView>>(`/servers/${sid}/${kind}`),
  musicSearch: (q: string) => get<MusicTrack[]>(`/music/search?q=${enc(q)}`),
  musicResolve: (url: string) => get<{ title: string; tracks: MusicTrack[] }>(`/music/resolve?url=${enc(url)}`),
  watch: (cid: string, kind: SessionKind = 'watch') => get<WatchView>(`/voice/${cid}/${kind}`),
  watchStart: (cid: string, video: string, queue?: string[], kind: SessionKind = 'watch') => send<void>('PUT', `/voice/${cid}/${kind}`, { video, queue }),
  draw: (cid: string) => get<DrawView>(`/voice/${cid}/draw`),
  drawJoin: (cid: string) => send<void>('PUT', `/voice/${cid}/draw`),
  drawStop: (cid: string) => send<void>('DELETE', `/voice/${cid}/draw`),
  drawSettings: (cid: string, b: Record<string, unknown>) => send<void>('PUT', `/voice/${cid}/draw/settings`, b),
  drawStart: (cid: string) => post<void>(`/voice/${cid}/draw/start`),
  drawChoose: (cid: string, index: number) => post<void>(`/voice/${cid}/draw/choose`, { index }),
  drawGuess: (cid: string, text: string) => post<void>(`/voice/${cid}/draw/guess`, { text }),
  drawOp: (cid: string, op: DrawOp) => post<void>(`/voice/${cid}/draw/draw`, op),
  drawVote: (cid: string, kind: 'report' | 'skip' | 'kick', target?: string) => post<void>(`/voice/${cid}/draw/vote`, { kind, target }),
  watchStop: (cid: string, kind: SessionKind = 'watch') => send<void>('DELETE', `/voice/${cid}/${kind}`),
  watchAction: (cid: string, action: string, b: { position?: number; rate?: number; rev?: number; video?: string } = {}, kind: SessionKind = 'watch') => post<void>(`/voice/${cid}/${kind}/action`, { action, ...b }),
  watchRepeat: (cid: string, repeat: '' | 'one' | 'all', kind: SessionKind = 'watch') => send<void>('PUT', `/voice/${cid}/${kind}/options`, { repeat }),
  watchReady: (cid: string, ready: boolean, kind: SessionKind = 'watch') => post<void>(`/voice/${cid}/${kind}/ready`, { ready }),
  watchHostOnly: (cid: string, host_only: boolean, kind: SessionKind = 'watch') => send<void>('PUT', `/voice/${cid}/${kind}/options`, { host_only }),
  watchEnqueue: (cid: string, video: string, kind: SessionKind = 'watch') => post<void>(`/voice/${cid}/${kind}/queue`, { video }),
  watchReorder: (cid: string, ids: string[], kind: SessionKind = 'watch') => send<void>('PUT', `/voice/${cid}/${kind}/queue`, { ids }),
  videos: (cid: string) => get<VideosBoard>(`/channels/${cid}/videos`),
  videoThumb: (cid: string, mid: string) => get<{ thumb: string }>(`/channels/${cid}/videos/posts/${mid}/thumb`),
  videoWatched: (cid: string, mid: string, watched: boolean) => send<void>('PUT', `/channels/${cid}/videos/posts/${mid}/watched`, { watched }),
  videoListCreate: (cid: string, name: string, collaborative = true) => post<VideoList>(`/channels/${cid}/videos/lists`, { name, collaborative }),
  videoListRename: (cid: string, lid: string, name: string, collaborative?: boolean) => send<void>('PUT', `/channels/${cid}/videos/lists/${lid}`, { name, collaborative }),
  videoListDelete: (cid: string, lid: string) => send<void>('DELETE', `/channels/${cid}/videos/lists/${lid}`),
  videoListItems: (cid: string, lid: string, ids: string[]) => send<void>('PUT', `/channels/${cid}/videos/lists/${lid}/items`, { ids }),
  sessionCreate: (cid: string, b: { title: string; starts_at: string; voice_channel_id: string; remind_minutes: number; list_id?: string; message_ids?: string[] }) => post<WatchSession>(`/channels/${cid}/videos/sessions`, b),
  sessionDelete: (cid: string, sid: string) => send<void>('DELETE', `/channels/${cid}/videos/sessions/${sid}`),
  sessionRsvp: (cid: string, sid: string, answer: Rsvp) => send<void>('PUT', `/channels/${cid}/videos/sessions/${sid}/rsvp`, { answer }),
  watchDequeue: (cid: string, id: string, kind: SessionKind = 'watch') => send<void>('DELETE', `/voice/${cid}/${kind}/queue/${id}`),
  serverVoice: (sid: string) => get<Record<string, VoiceParticipant[]>>(`/servers/${sid}/voice`),

  channelMessages: (cid: string, before?: string, limit = 50, after?: string) =>
    get<MessagePage>(`/channels/${cid}/messages?limit=${limit}${before ? `&before=${before}` : ''}${after ? `&after=${after}` : ''}`),
  sendChannelMessage: (cid: string, content: string, fileIds: string[] = [], replyTo?: string, poll?: NewPoll) =>
    post<Message>(`/channels/${cid}/messages`, { content, file_ids: fileIds, ...(replyTo ? { reply_to: replyTo } : {}), ...(poll ? { poll } : {}) }),
  dms: () => get<DM[]>('/dms'),
  openDM: (pseudo: string) => post<DM>('/dms', { pseudo }),
  dmMessages: (id: string, before?: string, limit = 50, after?: string) =>
    get<MessagePage>(`/dms/${id}/messages?limit=${limit}${before ? `&before=${before}` : ''}${after ? `&after=${after}` : ''}`),
  sendDMMessage: (id: string, content: string, fileIds: string[] = [], replyTo?: string, poll?: NewPoll) =>
    post<Message>(`/dms/${id}/messages`, { content, file_ids: fileIds, ...(replyTo ? { reply_to: replyTo } : {}), ...(poll ? { poll } : {}) }),
  pins: (key: string) => get<{ messages: Message[] }>(`${convPath(key)}/pins`),
  /** Recherche de messages ; `scope` : un seul de server_id, channel_id, dm_id (rien : partout). */
  search: (q: string, scope: { server_id?: string; channel_id?: string; dm_id?: string }, sort: 'relevance' | 'date', offset: number) =>
    get<MessagePage>(`/search?${new URLSearchParams({ q, sort, offset: String(offset), ...scope })}`),
  reads: (key: string) => get<Read[]>(`${convPath(key)}/reads`),
  typing: (key: string) => post<void>(`${convPath(key)}/typing`),
  vote: (id: string, optionId: string) => send<void>('PUT', `/messages/${id}/vote`, { option_id: optionId }),
  pollVotes: (id: string) => get<Record<string, Person[]>>(`/messages/${id}/poll/votes`),
  closePoll: (id: string) => send<void>('POST', `/messages/${id}/poll/close`),
  unvote: (id: string, optionId: string) => send<void>('DELETE', `/messages/${id}/vote`, { option_id: optionId }),
  pin: (id: string) => send<void>('PUT', `/messages/${id}/pin`),
  unpin: (id: string) => send<void>('DELETE', `/messages/${id}/pin`),
  emojis: () => get<CustomEmoji[]>('/emojis'),
  createEmoji: async (serverId: string, name: string, img: Blob) => {
    const res = await fetch(`${API_URL}/servers/${serverId}/emojis?name=${enc(name)}`, { method: 'POST', credentials: 'include', headers: { ...(await headers(false)), 'Content-Type': img.type }, body: img })
    if (!res.ok) {
      const e = await res.json().catch(() => ({}))
      throw new ApiError(res.status, e.error ?? 'http_' + res.status, e.message ?? res.statusText)
    }
  },
  renameEmoji: (serverId: string, id: string, name: string) => send<void>('PATCH', `/servers/${serverId}/emojis/${id}`, { name }),
  deleteEmoji: (serverId: string, id: string) => send<void>('DELETE', `/servers/${serverId}/emojis/${id}`),
  emojiBlobUrl: async (id: string) => {
    const res = await fetch(`${API_URL}/emojis/${id}/image`, { credentials: 'include', headers: await headers(false) })
    if (!res.ok) throw new ApiError(res.status, 'http_' + res.status, res.statusText)
    return URL.createObjectURL(await res.blob())
  },
  gifSearch: (q: string, offset = 0) => get<GifResult[]>(q ? `/gifs/search?q=${enc(q)}&offset=${offset}` : `/gifs/trending?offset=${offset}`),
  gifImport: (url: string) => post<Attachment>('/gifs/import', { url }),
  editMessage: (id: string, content: string, fileIds?: string[]) => send<Message>('PATCH', `/messages/${id}`, { content, file_ids: fileIds }),
  deleteMessage: (id: string) => send<void>('DELETE', `/messages/${id}`),
  react: (id: string, emoji: string) => send<void>('PUT', `/messages/${id}/reactions/${enc(emoji)}`),
  unreact: (id: string, emoji: string) => send<void>('DELETE', `/messages/${id}/reactions/${enc(emoji)}`),

  unread: () => get<{ channels: string[]; dms: string[]; counts: Record<string, number> }>('/unread'),
  markRead: (key: string) => post<void>(key.startsWith('c:') ? `/channels/${key.slice(2)}/read` : `/dms/${key.slice(2)}/read`),

  notifications: () => get<Mention[]>('/notifications'),
  readNotifications: (messageIds: string[] = []) => post<void>('/notifications/read', { message_ids: messageIds }),

  /** Envoie un fichier en attente d'un message ; `voice` pour un message vocal (durée en ms, forme d'onde). */
  upload: (file: Blob, name: string, voice?: { duration: number; waveform: number[] }) => {
    const fd = new FormData()
    if (voice) {
      fd.append('kind', 'voice')
      fd.append('duration_ms', String(voice.duration))
      fd.append('waveform', voice.waveform.join(','))
    }
    fd.append('file', file, name)
    return request<Attachment>('POST', '/files', fd)
  },
  /** Transcription d'un message vocal : calculée par le serveur (une fois, pour tous). */
  transcribe: (id: string) => post<{ transcript: string; transcript_auto: boolean }>(`/files/${id}/transcript`),
  /** Auteur seul : corrige la transcription, ou la retire (texte vide). */
  setTranscript: (id: string, text: string) => send<{ transcript: string; transcript_auto: boolean }>('PUT', `/files/${id}/transcript`, { text }),
  /**
   * Envoie une vidéo MP4 (replay) avec sa miniature JPEG ; XMLHttpRequest pour suivre la progression de l'envoi.
   * `signal` l'annule. Les erreurs réseau ont le statut 0 (voir `isTransient`).
   */
  uploadVideo: async (file: Blob, name: string, thumb: Blob | null, onProgress: (sent: number, total: number) => void, signal?: AbortSignal) => {
    const fd = new FormData()
    fd.append('kind', 'video')
    if (thumb) fd.append('thumb', thumb, 'thumb.jpg')
    fd.append('file', file, name)
    const h = await headers(false)
    return new Promise<Attachment>((resolve, reject) => {
      const x = new XMLHttpRequest()
      x.open('POST', `${API_URL}/files`)
      x.withCredentials = true
      for (const [k, v] of Object.entries(h)) x.setRequestHeader(k, v)
      x.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded, e.total)
      x.onload = () => {
        let body: { error?: string; message?: string } = {}
        try { body = JSON.parse(x.responseText) } catch { /* corps non JSON */ }
        if (x.status === 201) resolve(body as unknown as Attachment)
        else {
          if (x.status === 401) onUnauthorized?.()
          reject(new ApiError(x.status, body.error ?? 'http_' + x.status, body.message ?? x.statusText))
        }
      }
      x.onerror = x.ontimeout = () => reject(new ApiError(0, 'network', 'Connexion au serveur impossible.'))
      x.onabort = () => reject(new DOMException('annulé', 'AbortError'))
      signal?.addEventListener('abort', () => x.abort(), { once: true })
      if (signal?.aborted) return x.abort()
      x.send(fd)
    })
  },
  /** Jeton d'un lien signé d'une heure : <video> et <img> n'envoient pas l'en-tête Bearer (`?t=` sur `fileUrl` et `thumbUrl`). */
  fileToken: async (id: string) => (await post<{ token: string }>(`/files/${id}/link`)).token,
  thumbUrl: (id: string) => `${API_URL}/files/${id}/thumb`,
  fileUrl: (id: string) => `${API_URL}/files/${id}`,
  /** Télécharge un fichier avec l'authentification courante et renvoie une URL locale (mode Bearer). */
  fileBlobUrl: async (id: string) => {
    const res = await fetch(`${API_URL}/files/${id}`, { credentials: 'include', headers: await headers(false) })
    if (!res.ok) throw new ApiError(res.status, 'http_' + res.status, res.statusText)
    return URL.createObjectURL(await res.blob())
  },
  statuses: () => get<Record<string, Status>>('/statuses'),
  setActivity: (name: string) => send<void>('PUT', '/me/activity', { name }),
  setBanner: (color: string) => send<void>('PUT', '/me/banner', { color }),
  setStatus: (status: Status) => send<void>('PUT', '/me/status', { status }),
  notifySettings: () => get<NotifySettings>('/me/notify-settings'),
  setNotifySettings: (s: NotifySettings) => send<void>('PUT', '/me/notify-settings', s),
  avatars: () => get<Record<string, number>>('/avatars'),
  deleteAvatar: () => send<void>('DELETE', '/me/avatar'),
  setAvatar: async (img: Blob) => {
    const res = await fetch(`${API_URL}/me/avatar`, { method: 'PUT', credentials: 'include', headers: { ...(await headers(false)), 'Content-Type': img.type }, body: img })
    if (!res.ok) {
      const e = await res.json().catch(() => ({}))
      throw new ApiError(res.status, e.error ?? 'http_' + res.status, e.message ?? res.statusText)
    }
  },
  cameraBackground: async () => {
    const res = await fetch(`${API_URL}/me/camera-background`, { credentials: 'include', headers: await headers(false) })
    if (!res.ok) throw new ApiError(res.status, 'http_' + res.status, res.statusText)
    return res.blob()
  },
  deleteCameraBackground: () => send<void>('DELETE', '/me/camera-background'),
  setCameraBackground: async (img: Blob) => {
    const res = await fetch(`${API_URL}/me/camera-background`, { method: 'PUT', credentials: 'include', headers: { ...(await headers(false)), 'Content-Type': img.type }, body: img })
    if (!res.ok) {
      const e = await res.json().catch(() => ({}))
      throw new ApiError(res.status, e.error ?? 'http_' + res.status, e.message ?? res.statusText)
    }
  },
  serverAvatars: (sid: string) => get<Record<string, number>>(`/servers/${sid}/avatars`),
  deleteServerAvatar: (sid: string) => send<void>('DELETE', `/servers/${sid}/me/avatar`),
  setServerAvatar: async (sid: string, img: Blob) => {
    const res = await fetch(`${API_URL}/servers/${sid}/me/avatar`, { method: 'PUT', credentials: 'include', headers: { ...(await headers(false)), 'Content-Type': img.type }, body: img })
    if (!res.ok) {
      const e = await res.json().catch(() => ({}))
      throw new ApiError(res.status, e.error ?? 'http_' + res.status, e.message ?? res.statusText)
    }
  },
  deleteServerIcon: (sid: string) => send<void>('DELETE', `/servers/${sid}/icon`),
  setServerIcon: (sid: string, img: Blob) => putImage(`/servers/${sid}/icon`, img),
  serverIconBlobUrl: (sid: string) => imageUrl(`/servers/${sid}/icon`),
  updateBadge: (id: string, b: BadgeInput) => send<Badge>('PUT', `/admin/badges/${id}`, b),
  setBadgeIcon: (id: string, img: Blob) => putImage(`/admin/badges/${id}/icon`, img),
  deleteBadgeIcon: (id: string) => send<void>('DELETE', `/admin/badges/${id}/icon`),
  badgeIconBlobUrl: (id: string) => imageUrl(`/badges/${id}/icon`),
  grantBadges: (id: string, pseudos: string[]) => post<{ granted: number; missing: string[] }>(`/admin/badges/${id}/users`, { pseudos }),
  voiceStats: (sid: string) => get<Record<string, number>>(`/servers/${sid}/voice/stats`),
  avatarBlob: async (userId: string, serverId = '') => {
    const res = await fetch(`${API_URL}${serverId ? `/servers/${serverId}` : ''}/users/${userId}/avatar`, { credentials: 'include', headers: await headers(false) })
    if (!res.ok) throw new ApiError(res.status, 'http_' + res.status, res.statusText)
    return res.blob()
  },
  pushKey: () => get<{ public_key: string }>('/push/key'),
  pushTest: () => send<void>('POST', '/push/test'),
  pushSubscribe: (sub: PushSubscriptionJSON) => send<void>('PUT', '/push/subscription', sub),
  fcmSubscribe: (token: string) => send<void>('PUT', '/push/fcm', { token }),
  fcmUnsubscribe: (token: string) => send<void>('DELETE', '/push/fcm', { token }),
  pushUnsubscribe: (endpoint: string) => send<void>('DELETE', '/push/subscription', { endpoint }),
  linkPreview: (url: string) => get<LinkPreview>(`/link-preview?url=${enc(url)}`),

  wsTicket: () => post<{ ticket: string }>('/ws-ticket'),
  voiceToken: (cid: string) => post<VoiceToken>(`/voice/${cid}/token`),
  /** Jeton du participant `<userId>#screen` qui publie le partage d'écran natif (EST-18). */
  voiceScreenToken: (cid: string) => post<{ url: string; token: string; room: string }>(`/voice/${cid}/token?screen=1`),
  /** Coupe ou rétablit le micro et/ou le son d'un participant pour tout le salon (créateur seulement, EST-102). */
  voiceModerate: (cid: string, uid: string, mod: { mic_off: boolean; sound_off: boolean }) => request<void>('PUT', `/voice/${cid}/participants/${uid}/mod`, mod),

}


const estompeFilms = {
  library: (f: Record<string, string>) => get<{ items: FilmItem[]; total: number }>(`/films/library?${new URLSearchParams(f)}`),
  genres: () => get<{ genres: string[] }>('/films/genres'),
  resume: () => get<{ items: FilmItem[] }>('/films/continue'),
  seen: () => get<{ items: FilmItem[] }>('/films/seen'),
  item: (id: string) => get<FilmDetail>(`/films/items/${id}`),
  seasons: (id: string) => get<{ items: FilmItem[] }>(`/films/series/${id}/seasons`),
  episodes: (id: string) => get<{ items: FilmItem[] }>(`/films/seasons/${id}/episodes`),
  image: (id: string, type: 'poster' | 'fond', w: number) => imageUrl(`/films/items/${id}/image?type=${type}&w=${w}`),
  progress: (id: string, position: number, duration: number) => send<void>('PUT', `/films/items/${id}/progress`, { position, duration }),
  playback: (id: string, b: PlaybackRequest) => post<FilmPlayback>(`/films/items/${id}/playback`, { direct: b.direct, audio: b.audio }),
  close: (stream: string) => send<void>('DELETE', `/films/streams/${stream}`),
  closeOnExit: (stream: string) => { void fetch(`${API_URL}/films/streams/${stream}`, { method: 'DELETE', credentials: 'include', keepalive: true }).catch(() => {}) },
  rejects: () => get<{ items: FilmReject[] }>('/admin/films/rejets'),
  /** Base des URL de flux renvoyées par `playback`. */
  base: API_URL,
}

const eget = <T>(p: string) => requestAt<T>(ENTRACTE_URL, 'GET', p)
const esend = <T>(m: string, p: string, b?: unknown) => requestAt<T>(ENTRACTE_URL, m, p, b)
/** Saison d'une série d'Entracte vue comme un élément de la bibliothèque : `<id de la série>~<numéro>`. */
const SEP = '~'
const width = (w: number) => (w <= 342 ? 342 : w <= 500 ? 500 : 1280)
type SaisonEntracte = { number: number; title: string; episode_count: number; available_count: number; available: boolean; poster: boolean }
const normalise = (i: FilmItem): FilmItem => ({ ...i, duration: i.duration ?? 0, genres: i.genres ?? [] })
const liste = async (p: Promise<{ items: FilmItem[]; total?: number }>) => {
  const r = await p
  return { ...r, items: r.items.map(normalise), total: r.total ?? r.items.length }
}

/** Même interface que l'onglet films d'Estompe, servie par l'API d'Entracte. */
const entracteFilms: typeof estompeFilms = {
  library: (f) => liste(eget(`/library?${new URLSearchParams(f)}`)),
  genres: () => eget('/genres'),
  resume: () => liste(eget('/continue')),
  seen: () => liste(eget('/history?limit=100')),
  item: async (id) => normalise(await eget<FilmDetail>(`/titles/${enc(id)}`)) as FilmDetail,
  seasons: async (id) => {
    const r = await eget<{ items: SaisonEntracte[] }>(`/titles/${enc(id)}/seasons`)
    return {
      items: r.items.map((s) => ({
        id: `${id}${SEP}${s.number}`, kind: 'season' as const, title: s.title, year: null, overview: null, duration: 0, genres: [],
        season: s.number, episode: null, series_id: id, series_title: null, poster: s.poster, backdrop: false, progress: null, available: s.available,
      })),
    }
  },
  episodes: (sid) => {
    const [id, n] = sid.split(SEP)
    return liste(eget(`/titles/${enc(id)}/seasons/${enc(n ?? '')}/episodes`))
  },
  image: (id, type, w) => {
    const [titre, saison] = id.split(SEP)
    if (saison !== undefined) return imageAt(ENTRACTE_URL, `/titles/${enc(titre)}/seasons/${enc(saison)}/image`)
    return imageAt(ENTRACTE_URL, `/titles/${enc(id)}/image?type=${type === 'fond' ? 'backdrop' : 'poster'}&w=${width(w)}`)
  },
  progress: (id, position, duration) => esend<void>('PUT', `/titles/${enc(id)}/progress`, { position, duration }),
  playback: (id, b) => esend<FilmPlayback>('POST', `/titles/${enc(id)}/playback`, {
    audio: b.audio, quality: b.quality ?? 'original', start: b.start ?? 0, support: b.direct && b.support ? b.support : { containers: [], video: [], audio: [] },
  }),
  close: (stream) => esend<void>('DELETE', `/streams/${enc(stream)}`),
  closeOnExit: (stream) => { void fetch(`${ENTRACTE_URL}/streams/${enc(stream)}`, { method: 'DELETE', credentials: 'include', keepalive: true }).catch(() => {}) },
  rejects: () => eget('/admin/rejected'),
  base: ENTRACTE_URL,
}

/** Source de l'onglet films : Entracte dès que son URL est configurée, sinon le bloc films d'Estompe. */
export const filmsApi = ENTRACTE_URL ? entracteFilms : estompeFilms
export const entracteOn = () => !!ENTRACTE_URL

export const entracteApi = {
  me: () => eget<EntracteMe>('/me'),
  search: (q: string) => eget<{ items: SearchResult[]; tmdb_error: boolean }>(`/search?q=${enc(q)}`),
  series: (tmdbId: number) => eget<RequestableSeries>(`/search/series/${tmdbId}`),
  poster: (url: string) => imageAt(ENTRACTE_URL, url),
  request: (b: { kind: RequestKind; tmdb_id: number; seasons?: number[] }) => esend<EntracteRequest>('POST', '/requests', b),
  mine: () => eget<{ items: EntracteRequest[] }>('/requests/mine'),
  adminRequests: (f: Record<string, string>) => eget<{ items: EntracteRequest[] }>(`/admin/requests?${new URLSearchParams(Object.entries(f).filter(([, v]) => v))}`),
  accept: (id: string) => esend<EntracteRequest>('POST', `/admin/requests/${enc(id)}/accept`),
  refuse: (id: string, reason: string) => esend<EntracteRequest>('POST', `/admin/requests/${enc(id)}/refuse`, { reason }),
  status: () => eget<EntracteStatus>('/admin/status'),
  sync: () => esend<void>('POST', '/admin/sync'),
  pushKey: () => eget<{ key: string }>('/push/key'),
  pushSubscribe: (sub: PushSubscriptionJSON) => esend<void>('PUT', '/push/subscription', sub),
  pushUnsubscribe: (endpoint: string) => esend<void>('DELETE', '/push/subscription', { endpoint }),
  fcmSubscribe: (token: string) => esend<void>('PUT', '/push/fcm', { token }),
  notifications: (after: number) => eget<{ items: EntracteNotification[]; last_id: number }>(`/notifications?after=${after}`),
}
