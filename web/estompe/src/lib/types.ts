export interface Person { id: string; pseudo: string }
export interface AdminLive {
  connections: number
  online: { pseudo: string; platform: string; version: string }[]
  voice: { server: string; channel: string; participants: { user_id: string; pseudo: string; screen?: boolean; camera?: boolean }[] }[]
}
export interface AdminOverview {
  version: string
  servers: { id: string; name: string; members: number; channels: number; messages: number; last_message_at: string | null }[]
  config: {
    settings: { push_web: boolean; push_android: boolean; giphy: boolean; metrics: boolean; allow_video: boolean; livekit_url: string; max_storage: number; origins: string[] | null }
    max_file_size: number; max_participants: number; screen_max_fps?: number; screen_max_height?: number
  }
}
/** icon_version : 0 = emoji `icon` ; sinon image de GET /badges/{id}/icon. */
export interface Badge { id: string; name: string; icon: string; color: string; description: string; icon_version: number; sort_order: number; created_at: string }
export type BadgeInput = Pick<Badge, 'name' | 'color' | 'description' | 'sort_order'>
export interface AdminBadge extends Badge { holders: { id: string; pseudo: string }[] }
export interface AdminUser { id: string; pseudo: string; created_at: string; suspended: boolean; servers: number; messages: number }
export interface AdminStats {
  users: number; new_users_7d: number; suspended: number; servers: number; messages: number; messages_24h: number
  files: number; storage_bytes: number; errors_24h: number
}
export interface AdminLogEntry { at: string; actor: string; action: string; target: string }
export type ReportStatus = 'new' | 'in_progress' | 'resolved' | 'rejected' | 'duplicate'
export interface Report {
  id: string; created_at: string; title: string; category: string; platform: string; status: ReportStatus; duplicate_of: string | null
  author?: string; seen?: boolean; description?: string; tech?: Record<string, string>; logs?: string; notes?: string; attachments?: Attachment[]
}
export interface ReportInput { title: string; description: string; category: string; platform: string; tech: Record<string, string>; logs: string; file_ids: string[] }
export interface AdminError { at: string; method: string; path: string; status: number }
export interface Capacite { allow: string[]; deny: string[] }
export type Capacites = Record<string, Capacite>
export interface Me { activated: boolean; allowed: boolean; admin?: boolean; rights?: ('access' | 'admin' | 'movies' | 'acces' | 'users')[]; capabilities?: Capacites; id?: string; pseudo?: string; sub?: string; movies?: boolean; max_video_size?: number; transcription?: '' | 'alpha' | 'on'; blocks?: string[] }
/** `icon_version` : 0 sans icône ; change à chaque modification. */
export interface Server { id: string; name: string; owner_id: string; icon_version: number }
/** Clés de permission, dans l'ordre de l'API (voir api/estompe/openapi.yaml, schéma Permissions). */
export type PermKey =
  | 'manage_server' | 'manage_roles' | 'manage_channels' | 'create_invites' | 'view_audit_log'
  | 'kick_members' | 'ban_members' | 'manage_nicknames'
  | 'view_channel' | 'send_messages' | 'attach_files' | 'add_reactions' | 'mention_everyone' | 'manage_messages' | 'create_polls'
  | 'connect' | 'speak' | 'video' | 'share_screen' | 'mute_members' | 'deafen_members' | 'move_members'
  | 'timeout_members' | 'disconnect_members' | 'organize_watch'
/** `position` : 0 pour « @tout le monde » (`is_default`), plus haut = plus élevé. `color` vide = sans couleur ; `icon` : emoji facultatif. */
export interface Role { id: string; name: string; color: string; icon: string; position: number; hoist: boolean; permissions: PermKey[]; is_default: boolean }
export interface RoleInput { name: string; color: string; icon: string; hoist: boolean; permissions: PermKey[] }
/** Exception d'un rôle sur un salon : permissions autorisées ou refusées (les autres sont héritées). */
export interface Override { role_id: string; allow: PermKey[]; deny: PermKey[] }
/** `actor_id`, `target_id` et `reason` : renseignés pour les actions de modération. */
export interface AuditEntry { at: string; actor: string; actor_id?: string; action: string; target: string; target_id?: string; reason?: string }
export interface Ban { user_id: string; pseudo: string; by: string; reason: string; at: string; expires_at: string | null }
export type ChannelKind = 'text' | 'voice' | 'videos' | 'music' | 'cinema'
export const isVoiceKind = (kind: ChannelKind) => kind === 'voice' || kind === 'cinema'
/** `permissions` : les vôtres dans le salon (rôles + exceptions). */
export interface Channel { id: string; name: string; kind: ChannelKind; position: number; permissions?: PermKey[]; slowmode?: number; locked?: boolean; topic?: string; user_limit?: number }
export interface NewChannel { name: string; kind: ChannelKind; topic?: string; slowmode?: number; user_limit?: number; after?: string; private?: boolean; roles?: string[]; members?: string[] }
/** `roles` : identifiants des rôles du membre, du plus élevé au plus bas, sans « @tout le monde ». */
/** `nickname` : pseudo de serveur (vide sans) ; `timeout_until` : fin de l'exclusion temporaire en cours ; `joined_at` / `created_at` : arrivée sur le serveur / sur Estompe ; `banner_color` : « #rrggbb » choisi pour la bannière du profil. */
export interface Member extends Person { online: boolean; roles?: string[]; nickname?: string; timeout_until?: string | null; joined_at?: string; created_at?: string; banner_color?: string }
export interface ServerDetail { server: Server; channels: Channel[]; members: Member[]; roles?: Role[]; permissions?: PermKey[] }
export interface ServerInvite { code: string; created_at: string; created_by?: string; uses?: number; max_uses?: number | null; expires_at?: string | null }
/** Dernier message vu par une personne dans une conversation (`last_id` : id du message). */
export interface Read { user_id: string; pseudo: string; last_id: string; read_at: string }
export interface Attachment { id: string; name: string; mime: string; size: number; expired: boolean; kind: 'file' | 'voice' | 'video'; duration_ms?: number; waveform?: number[]; thumb?: boolean; transcript?: string; transcript_auto?: boolean }
export interface Reaction { emoji: string; count: number; me: boolean; users: Person[] }
export interface NewPoll { options: string[]; multiple: boolean; closes_at?: string }
export interface PollOption { id: string; label: string; votes: number; me: boolean }
export interface Message {
  id: string
  channel_id?: string
  dm_id?: string
  author: Person
  content: string
  created_at: string
  edited_at: string | null
  attachments: Attachment[]
  reactions: Reaction[]
  mentions: string[]
  reply_to?: { id: string; author: Person; content: string } | null
  pinned_at?: string | null
  system?: boolean // message publié par le serveur (résultat de sondage d'avant la carte)
  poll?: { multiple: boolean; closes_at: string | null; created_by: Person; options: PollOption[] } | null
}
export type Status = 'online' | 'idle' | 'dnd'
/** Évènement WebSocket `notification` : contenu décidé par le serveur pour l'app ouverte (même forme que le push). */
export interface ServerNotification {
  title: string; body: string; tag: string; url: string
  conv?: string; msg?: string
  kind: 'dm' | 'mention' | 'channel' | 'voice'
  silent: boolean
}
export type NotifyLevel = 'all' | 'mentions' | 'none'
/** Réglages de notifications, stockés côté serveur (identiques sur tous les appareils). `levels` : serveur, salon ou conversation privée → niveau ; absent = défaut. */
export interface NotifySettings {
  levels: Record<string, NotifyLevel>
  types: { dm: boolean; mention: boolean; channel: boolean; voice: boolean }
  preview: boolean
  sound: boolean
  ignore_group?: string[]
}
export const defaultNotifySettings = (): NotifySettings => ({ levels: {}, types: { dm: true, mention: true, channel: true, voice: true }, preview: true, sound: true })
export interface CustomEmoji { id: string; server_id: string; name: string; created_by: string }
export interface GifResult { id: string; title: string; preview: string; url: string; width: number; height: number }
export interface MessagePage { messages: Message[]; has_more: boolean }
export interface DM { id: string; other: Person; online: boolean }
export interface Mention {
  message_id: string
  channel_id?: string
  server_id?: string
  dm_id?: string
  author: Person
  content: string
  created_at: string
  group?: boolean
}
export interface LinkPreview { url: string; title: string; description?: string; image?: string; site_name?: string }
export interface VoiceParticipant { user_id: string; pseudo: string; screen?: boolean; camera?: boolean; since?: string; mic_off?: boolean; sound_off?: boolean }
/** Music2gether (EST-160) : morceau YouTube Music décrit par le serveur ; `cover` est un chemin de l'API (GET /music/cover/{id}). */
export interface MusicMeta { title: string; artist: string; album: string; duration: number; cover: string }
/** Résultat de `GET /music/search` : un morceau décrit comme ceux d'une session. */
export interface MusicTrack extends MusicMeta { id: string }
export interface WatchItem { id: string; provider: string; video: string; by: string; by_id: string; meta?: MusicMeta }
/** Session « Regarder ensemble » (EST-143) : position courante = pos + (heure − at) × rate si playing. */
export interface WatchState {
  rev: number; provider: string; video: string; playing: boolean; pos: number; rate: number; at: string
  host: string; host_only: boolean; by_id: string; by: string; act: string
  queue: WatchItem[]; ready: Record<string, boolean>
  meta?: MusicMeta; repeat: '' | 'one' | 'all'; history: WatchItem[]
}
/** Salon « Vidéos » (EST-149). `provider` : youtube, twitch_vod, twitch_clip, twitch_live, vimeo, dailymotion, file, page, simple (carte sans lecteur). */
export interface VideoPost {
  message_id: string; author: Person; created_at: string; provider: string; ref: string; url: string
  title: string; site: string; has_thumb: boolean; watched_at: string | null; reactions: number; liked: boolean; replies: number
}
/** `collaborative` : tout membre qui écrit dans le salon peut modifier la liste ; sinon seulement son créateur. */
export interface VideoList { id: string; name: string; created_by: string; collaborative: boolean; items: string[] }
export type Rsvp = 'yes' | 'maybe' | 'no'
export interface WatchSession {
  id: string; title: string; starts_at: string; voice_channel_id: string; remind_minutes: number
  videos: string[]; created_by: string; started_at: string | null; rsvps: Record<string, Rsvp>
}
export interface VideosBoard { posts: VideoPost[]; lists: VideoList[]; sessions: WatchSession[] }
/** Les deux sessions d'un salon vocal, indépendantes : « Regarder ensemble » (`watch`) et Music2gether (`music`, EST-168) ; `cinema` : la séance d'un salon cinéma. */
export type SessionKind = 'watch' | 'music' | 'cinema'
export interface WatchView { state: WatchState | null; now: string }
export interface VoiceToken { url: string; token: string; room: string; max_participants: number; screen_max_fps?: number; screen_max_height?: number }

/** Clé d'une conversation : "c:<salon>" ou "d:<conversation privée>". */
export type ConvKey = string
export const channelKey = (id: string): ConvKey => `c:${id}`
export const dmKey = (id: string): ConvKey => `d:${id}`
export const convKeyOf = (m: { channel_id?: string; dm_id?: string }): ConvKey =>
  m.channel_id ? channelKey(m.channel_id) : dmKey(m.dm_id as string)
