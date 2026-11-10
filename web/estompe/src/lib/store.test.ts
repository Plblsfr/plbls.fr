import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const unauthorized = vi.hoisted(() => ({ fn: null as null | (() => void) }))
const ApiErrorMock = vi.hoisted(() => class extends Error { status = 0 })
const entracteMock = vi.hoisted(() => ({ on: false, me: null as null | { sub: string; name: string; email: string; permissions: { access: boolean; request: boolean; admin: boolean }; push: { web: boolean; fcm: boolean } } }))
vi.mock('./api', () => ({
  api: { videos: vi.fn(async () => ({ posts: [], lists: [], sessions: [] })), channelMessages: vi.fn(), deleteMessage: vi.fn(async () => {}), readNotifications: vi.fn(async () => {}), markRead: vi.fn(async () => {}), notifySettings: vi.fn(async () => ({ levels: {}, types: { dm: true, mention: true, channel: true, voice: true }, preview: true, sound: true })), setNotifySettings: vi.fn(async () => {}), reads: vi.fn(async () => []), unread: vi.fn(async () => ({ channels: [], dms: [] })), avatars: vi.fn(async () => ({})), statuses: vi.fn(async () => ({})), notifications: vi.fn(async () => []), dms: vi.fn(async () => []), servers: vi.fn(async () => []), server: vi.fn(async () => ({ server: { id: 's1', name: 'Amis', owner_id: 'me', icon_version: 0 }, channels: [{ id: 'c1', name: 'général', kind: 'text', position: 1, permissions: ['view_channel'] }], members: [], roles: [], permissions: ['view_channel'] })) },
  ApiError: ApiErrorMock,
  API_URL: '',
  setUnauthorizedHandler: (h: () => void) => { unauthorized.fn = h },
  entracteOn: () => entracteMock.on,
  entracteApi: { me: vi.fn(async () => { if (!entracteMock.me) throw new Error('sans session'); return entracteMock.me }), fcmSubscribe: vi.fn(async () => {}), pushSubscribe: vi.fn(async () => {}), notifications: vi.fn(async () => ({ items: [], last_id: 0 })) },
}))
vi.mock('./realtime', () => ({ Realtime: class { start() {} stop() {} }, wsUrl: () => '' }))

import { AppStore } from './store.svelte'
import { platform as web } from '@platform'
import { setPlatform } from './platform'
import { api } from './api'
import { avatars } from '../blocks/avatars/avatars.svelte'
import { voice } from '../blocks/vocal/voice.svelte'
import { videos } from '../blocks/videos/videos.svelte'
import { draw } from '../blocks/draw2gether/draw.svelte'
import { pollWinners } from './poll'
import { channelNameError } from './channel-create.svelte'
import { BOT_ID, formatDuration, formatVoiceTotal, isBot, oldestSince, voiceTimes } from './format'
import { isVoiceKind, type ChannelKind, type Message } from './types'

const msg = (o: Partial<Message> = {}): Message => ({
  id: '1', channel_id: 'c1', author: { id: 'bob', pseudo: 'bob' }, content: 'salut',
  created_at: '2026-10-13T10:00:00Z', edited_at: null, attachments: [], reactions: [], mentions: [], ...o,
})

let s: AppStore
beforeEach(() => {
  s = new AppStore()
  s.meId = 'me'
  s.pseudo = 'moi'
  s.servers = [{ id: 's1', name: 'Amis', owner_id: 'me', icon_version: 0 }]
  s.details = { s1: { server: s.servers[0], channels: [{ id: 'c1', name: 'général', kind: 'text', position: 1 }], members: [{ id: 'me', pseudo: 'moi', online: true }, { id: 'bob', pseudo: 'bob', online: false }] } }
  s.convs = { 'c:c1': { messages: [], hasMore: false, loading: false, loaded: true } }
})
const ev = (type: string, data: unknown) => s.handleEvent({ type, data })

describe('accès aux films', () => {
  const avec = async (on: boolean, access: boolean) => {
    entracteMock.on = on
    entracteMock.me = { sub: 'kc-me', name: 'moi', email: '', permissions: { access, request: false, admin: false }, push: { web: false, fcm: false } }
    Object.assign(api, { me: vi.fn(async () => ({ activated: true, allowed: true, id: 'me', pseudo: 'moi', movies: true, blocks: ['films', 'vocal', 'regarder-ensemble'] })) })
    await s.boot().catch(() => {})
    return s.movies
  }
  afterEach(() => { entracteMock.on = false; entracteMock.me = null })
  it("suit la permission d'Entracte, jamais l'ancien champ `movies` de /me", async () => {
    expect(await avec(true, true)).toBe(true)
    expect(await avec(true, false)).toBe(false)
    expect(await avec(false, true)).toBe(false)
  })
  it('salon cinéma seulement quand /me l’annonce', async () => {
    await avec(true, true)
    expect(s.cinemaOn).toBe(false)
  })
})

describe('droits annoncés par /me', () => {
  const boot = async (me: Record<string, unknown>) => {
    Object.assign(api, { me: vi.fn(async () => me) })
    await s.boot().catch(() => {})
  }
  it("ouvre l'administration seule à un administrateur des permissions sans accès à Estompe", async () => {
    await boot({ activated: false, allowed: false, admin: false, rights: ['permissions'] })
    expect(s.phase).toBe('admin')
    expect(s.adminSections.map((x) => x.id)).toEqual(['permissions'])
    expect(s.isAdmin).toBe(false)
  })
  it("refuse l'accès sans aucun droit d'administration", async () => {
    await boot({ activated: false, allowed: false, admin: false, rights: [] })
    expect(s.phase).toBe('denied')
  })
  it('déduit les droits des anciens champs sans `rights`', async () => {
    await boot({ activated: false, allowed: false, admin: true })
    expect(s.phase).toBe('admin')
    expect(s.adminSections.some((x) => x.id === 'permissions')).toBe(false)
    expect(s.isAdmin).toBe(true)
  })
})

describe('sections selon les permissions (Entracte)', () => {
  const droits = (access: boolean, request = false, admin = false) => ({ sub: 'kc-me', name: 'moi', email: '', permissions: { access, request, admin }, push: { web: false, fcm: false } })
  const boot = async (me: Record<string, unknown> | 'refus', entracte: ReturnType<typeof droits> | null) => {
    entracteMock.on = true
    entracteMock.me = entracte
    Object.assign(api, { me: vi.fn(async () => { if (me === 'refus') throw Object.assign(new (vi.mocked(ApiErrorMock))(), { status: 401 }); return me }) })
    vi.mocked(api.servers).mockClear()
    history.replaceState(null, '', '#/')
    await s.boot().catch(() => {})
  }
  afterEach(() => { entracteMock.on = false; entracteMock.me = null })

  it('Entracte seul : section Entracte, sans messagerie ni appel à ses routes', async () => {
    await boot({ activated: false, allowed: false, admin: false, rights: [] }, droits(true))
    expect(s.phase).toBe('entracte')
    expect(s.movies).toBe(true)
    expect(api.servers).not.toHaveBeenCalled()
    expect(location.hash).toBe('#/films')
  })
  it('Estompe seul : messagerie, aucune entrée Entracte', async () => {
    await boot({ activated: true, allowed: true, id: 'me', pseudo: 'moi', blocks: ['films', 'vocal', 'regarder-ensemble'], movies: true }, droits(false))
    expect(s.phase).toBe('ready')
    expect(s.movies).toBe(false)
  })
  it('Estompe et Entracte : messagerie, la section Entracte remplace l’onglet films', async () => {
    await boot({ activated: true, allowed: true, id: 'me', pseudo: 'moi', blocks: [], movies: false, cinema: true }, droits(true, true))
    expect(s.phase).toBe('ready')
    expect(s.movies).toBe(true)
    expect(s.cinemaOn).toBe(true)
    expect(s.entracteMe?.permissions.request).toBe(true)
  })
  it('aucune des deux : page « aucun accès »', async () => {
    await boot({ activated: false, allowed: false, admin: false, rights: [] }, droits(false))
    expect(s.phase).toBe('denied')
    await boot({ activated: false, allowed: false, admin: false, rights: [] }, null)
    expect(s.phase).toBe('denied')
  })
  it('Entracte joignable quand Estompe refuse la session : section Entracte ; sinon connexion', async () => {
    await boot('refus', droits(true))
    expect(s.phase).toBe('entracte')
    await boot('refus', null)
    expect(s.phase).toBe('login')
  })
})

describe('loadFrom', () => {
  const page = (ids: string[], has_more: boolean) => ({ messages: ids.map((id) => msg({ id })), has_more })
  it('remplace l\'historique par les messages depuis le message visé jusqu\'au présent, en gardant ceux arrivés entre-temps', async () => {
    s.convs['c:c1'].messages = [msg({ id: '99' }), msg({ id: '101' })]
    vi.mocked(api.channelMessages).mockResolvedValueOnce(page(['5', '6'], true)).mockResolvedValueOnce(page(['7'], false))
    expect(await s.loadFrom('c:c1', '5')).toBe(true)
    expect(vi.mocked(api.channelMessages).mock.calls.map((c) => c[3])).toEqual(['4', '6'])
    expect(s.convs['c:c1'].messages.map((m) => m.id)).toEqual(['5', '6', '7', '99', '101'])
    expect(s.convs['c:c1'].hasMore).toBe(true)
  })
  it('laisse l\'historique intact si le message n\'existe plus', async () => {
    s.convs['c:c1'].messages = [msg({ id: '99' })]
    vi.mocked(api.channelMessages).mockResolvedValueOnce(page(['7'], false))
    expect(await s.loadFrom('c:c1', '5')).toBe(false)
    expect(s.convs['c:c1'].messages.map((m) => m.id)).toEqual(['99'])
  })
})

describe('messages', () => {
  it('ajoute un message reçu une seule fois, même s\'il est aussi renvoyé par l\'envoi', () => {
    ev('message.created', msg())
    ev('message.created', msg())
    expect(s.convs['c:c1'].messages).toHaveLength(1)
  })
  it('ne charge rien pour une conversation jamais ouverte', () => {
    ev('message.created', msg({ channel_id: 'c2' }))
    expect(s.convs['c:c2']).toBeUndefined()
    expect(s.unread['c:c2']).toBe(1) // mais la pastille apparaît
  })
  it('marque non lu seulement hors de la conversation ouverte et jamais ses propres messages', () => {
    s.current = 'c:c1'
    ev('message.created', msg({ id: '1' }))
    expect(s.unread['c:c1']).toBeUndefined()
    s.current = null
    ev('message.created', msg({ id: '2', author: { id: 'me', pseudo: 'moi' } }))
    expect(s.unread['c:c1']).toBeUndefined()
    ev('message.created', msg({ id: '3' }))
    expect(s.unread['c:c1']).toBe(1)
  })
  it('affiche la notification décidée par le serveur', async () => {
    const notify = await import('../blocks/notifications/notify')
    const show = vi.spyOn(notify, 'showNotification').mockImplementation(() => {})
    s.current = 'c:c1'
    ev('notification', { title: 'a', body: '', tag: 't', url: '/#/s/s1/c1', conv: 'c:c1', msg: '1', kind: 'channel', silent: false })
    expect(show).toHaveBeenCalledWith(expect.objectContaining({ tag: 't' }), true, '')
    show.mockRestore()
  })
  it('applique les réglages de notifications reçus d\'un autre appareil', () => {
    ev('notify_settings.updated', { levels: { s1: 'none' }, types: { dm: true, mention: true, channel: false, voice: true }, preview: false, sound: false })
    expect(s.notifySettings.levels.s1).toBe('none')
    expect(s.notifySettings.preview).toBe(false)
  })
  it('enregistre les réglages et les rétablit en cas d\'erreur', async () => {
    const { api } = await import('./api')
    await s.saveNotifySettings({ sound: false })
    expect(s.notifySettings.sound).toBe(false)
    expect(api.setNotifySettings).toHaveBeenLastCalledWith(expect.objectContaining({ sound: false, preview: true }))
    vi.mocked(api.setNotifySettings).mockRejectedValueOnce(new Error('x'))
    await expect(s.saveNotifySettings({ preview: false })).rejects.toThrow()
    expect(s.notifySettings.preview).toBe(true)
  })
  it('conserve ses propres réactions lors d\'une modification', () => {
    s.convs['c:c1'].messages = [msg({ reactions: [{ emoji: '👍', count: 2, me: true, users: [] }] })]
    // l'évènement porte les réactions vues par l'auteur de la modification : elles ne doivent pas écraser les nôtres
    ev('message.updated', msg({ content: 'modifié', edited_at: 'x', reactions: [{ emoji: '👍', count: 2, me: false, users: [] }] }))
    const m = s.convs['c:c1'].messages[0]
    expect(m.content).toBe('modifié')
    expect(m.reactions[0].me).toBe(true)
  })
  it('retire le message dès que l\'API confirme la suppression, et le garde en cas d\'erreur', async () => {
    const { api } = await import('./api')
    s.convs['c:c1'].messages = [msg()]
    vi.mocked(api.deleteMessage).mockRejectedValueOnce(new Error('non'))
    await expect(s.deleteMessage('c:c1', '1')).rejects.toThrow()
    expect(s.convs['c:c1'].messages).toHaveLength(1)
    await s.deleteMessage('c:c1', '1')
    expect(s.convs['c:c1'].messages).toHaveLength(0)
  })
  it('retire un message supprimé et ses mentions', () => {
    s.convs['c:c1'].messages = [msg()]
    s.mentions = [{ message_id: '1', channel_id: 'c1', author: { id: 'bob', pseudo: 'bob' }, content: '', created_at: '' }]
    ev('message.deleted', { id: '1', channel_id: 'c1' })
    expect(s.convs['c:c1'].messages).toHaveLength(0)
    expect(s.mentions).toHaveLength(0)
  })
})

describe('perte de session', () => {
  it('quitte le vocal et affiche la connexion', () => {
    const leave = vi.spyOn(voice, 'leave').mockResolvedValue()
    s.phase = 'ready'
    unauthorized.fn!()
    expect(s.phase).toBe('login')
    expect(leave).toHaveBeenCalledTimes(1)
  })
})

describe('accusés de lecture', () => {
  const read = (user_id: string, last_id: string, read_at = '2026-10-13T10:05:00Z') => ev('read.updated', { channel_id: 'c1', user_id, pseudo: user_id, last_id, read_at })
  beforeEach(() => {
    s.convs['c:c1'].messages = [msg({ id: '9' }), msg({ id: '10' }), msg({ id: '11' })]
  })
  it('place chaque personne sous son dernier message vu, sans soi-même', () => {
    read('bob', '10')
    read('me', '11')
    expect(Object.keys(s.readers('c:c1'))).toEqual(['10'])
  })
  it('déplace l\'avatar vers un message plus récent (ids comparés comme des entiers)', () => {
    read('bob', '9')
    expect(Object.keys(s.readers('c:c1'))).toEqual(['9'])
    read('bob', '11')
    expect(Object.keys(s.readers('c:c1'))).toEqual(['11'])
  })
  it('retombe sur le message chargé précédent si le dernier vu n\'est plus affiché', () => {
    read('bob', '100')
    expect(Object.keys(s.readers('c:c1'))).toEqual(['11'])
  })
})

describe('lecture depuis un autre appareil', () => {
  it('efface la pastille et les mentions de la conversation quand mon compte la lit ailleurs', () => {
    s.unread['c:c1'] = 3
    s.mentions = [{ message_id: '5', channel_id: 'c1', server_id: 's1', author: { id: 'bob', pseudo: 'bob' }, content: 'yo', created_at: '' } as never]
    ev('read.updated', { channel_id: 'c1', user_id: 'bob', pseudo: 'bob', last_id: '5', read_at: '' })
    expect(s.unread['c:c1']).toBe(3)
    ev('read.updated', { channel_id: 'c1', user_id: 'me', pseudo: 'moi', last_id: '5', read_at: '' })
    expect(s.unread['c:c1']).toBeUndefined()
    expect(s.totalMentions).toBe(0)
  })
})

describe('notification de la plateforme', () => {
  it('retire la notification de la conversation qu\'on ouvre ou qu\'on lit', async () => {
    const cancel = vi.fn()
    setPlatform({ ...web, cancelNotification: cancel })
    s.readConv('c:c1')
    expect(cancel).toHaveBeenCalledWith('c:c1')
    vi.spyOn(document, 'hasFocus').mockReturnValue(true)
    await s.open('c:c1')
    expect(cancel).toHaveBeenCalledTimes(2)
    setPlatform(web)
  })
})

describe('réactions', () => {
  beforeEach(() => { s.convs['c:c1'].messages = [msg()] })
  it('compte les réactions des autres et distingue les siennes', () => {
    ev('reaction.updated', { message_id: '1', emoji: '👍', user_id: 'bob', pseudo: 'bob', added: true })
    ev('reaction.updated', { message_id: '1', emoji: '👍', user_id: 'me', added: true })
    expect(s.convs['c:c1'].messages[0].reactions).toEqual([{ emoji: '👍', count: 2, me: true, users: [{ id: 'bob', pseudo: 'bob' }, { id: 'me', pseudo: '' }] }])
    ev('reaction.updated', { message_id: '1', emoji: '👍', user_id: 'bob', added: false })
    expect(s.convs['c:c1'].messages[0].reactions).toEqual([{ emoji: '👍', count: 1, me: true, users: [{ id: 'me', pseudo: '' }] }])
    ev('reaction.updated', { message_id: '1', emoji: '👍', user_id: 'me', added: false })
    expect(s.convs['c:c1'].messages[0].reactions).toEqual([])
  })
  it('ne compte pas deux fois sa propre réaction (mise à jour locale puis évènement)', () => {
    s.convs['c:c1'].messages[0].reactions = [{ emoji: '👍', count: 1, me: true, users: [{ id: 'me', pseudo: 'moi' }] }]
    ev('reaction.updated', { message_id: '1', emoji: '👍', user_id: 'me', added: true })
    expect(s.convs['c:c1'].messages[0].reactions[0].count).toBe(1)
    s.convs['c:c1'].messages[0].reactions = []
    ev('reaction.updated', { message_id: '1', emoji: '❤️', user_id: 'me', added: false })
    expect(s.convs['c:c1'].messages[0].reactions).toEqual([])
  })
})

describe('sondages', () => {
  it('reprend les totaux de l\'évènement et ne marque « me » que pour son propre vote', () => {
    s.convs['c:c1'].messages = [{ ...msg(), poll: { multiple: false, closes_at: null, created_by: { id: 'bob', pseudo: 'bob' }, options: [{ id: '5', label: 'a', votes: 0, me: false }, { id: '6', label: 'b', votes: 0, me: false }] } }]
    ev('poll.voted', { message_id: '1', user_id: 'bob', options: ['5'], counts: { '5': 1, '6': 0 } })
    expect(s.convs['c:c1'].messages[0].poll!.options.map((o) => [o.votes, o.me])).toEqual([[1, false], [0, false]])
    ev('poll.voted', { message_id: '1', user_id: 'me', options: ['6'], counts: { '5': 1, '6': 1 } })
    expect(s.convs['c:c1'].messages[0].poll!.options.map((o) => [o.votes, o.me])).toEqual([[1, false], [1, true]])
  })
})

describe('résultat de sondage', () => {
  const opt = (id: string, votes: number) => ({ id, label: id, votes, me: false })
  it('met en avant les choix en tête, ex æquo compris, et aucun sans vote', () => {
    expect(pollWinners([opt('a', 2), opt('b', 3), opt('c', 3)])).toEqual(['b', 'c'])
    expect(pollWinners([opt('a', 0), opt('b', 0)])).toEqual([])
  })
})

describe('clôture de sondage', () => {
  it('marque le sondage clos', () => {
    s.convs['c:c1'].messages = [{ ...msg(), poll: { multiple: false, closes_at: null, created_by: { id: 'bob', pseudo: 'bob' }, options: [] } }]
    ev('poll.closed', { message_id: '1', closes_at: '2026-01-01T00:00:00Z' })
    expect(s.convs['c:c1'].messages[0].poll!.closes_at).toBe('2026-01-01T00:00:00Z')
  })
})

describe('serveurs, membres et présence', () => {
  it('suit la présence', () => {
    ev('presence.snapshot', { online: ['bob'] })
    expect(s.online.bob).toBe(true)
    ev('presence', { user_id: 'bob', online: false })
    expect(s.online.bob).toBeUndefined()
  })
  it('suit le jeu en cours des autres', () => {
    ev('presence.snapshot', { online: ['bob'], activities: { bob: 'PokeMMO' } })
    expect(s.activityOf('bob')).toBe('PokeMMO')
    ev('activity.updated', { user_id: 'bob', name: null })
    expect(s.activityOf('bob')).toBe('')
    ev('activity.updated', { user_id: 'bob', name: 'Factorio' })
    ev('presence', { user_id: 'bob', online: false })
    expect(s.activityOf('bob')).toBe('')
  })
  it('affiche un arrivant déjà en ligne comme tel', () => {
    ev('member.joined', { server_id: 's1', member: { id: 'carol', pseudo: 'carol', online: true } })
    expect(s.details.s1.members.find((m) => m.id === 'carol')?.online).toBe(true)
    expect(s.online.carol).toBe(true)
    ev('member.joined', { server_id: 's1', member: { id: 'carol', pseudo: 'carol', online: true } })
    expect(s.details.s1.members.filter((m) => m.id === 'carol')).toHaveLength(1)
  })
  it('retire un membre parti, et le serveur quand on le quitte', () => {
    ev('member.left', { server_id: 's1', user_id: 'bob' })
    expect(s.details.s1.members.map((m) => m.id)).toEqual(['me'])
    ev('member.left', { server_id: 's1', user_id: 'me' })
    expect(s.details.s1).toBeUndefined()
    expect(s.servers).toHaveLength(0)
  })
  it('signale une expulsion ou un bannissement avec la raison', async () => {
    const ui = await import('./ui')
    const fail = vi.spyOn(ui.notify, 'fail').mockImplementation(() => 0)
    ev('member.left', { server_id: 's1', user_id: 'me', kind: 'ban', reason: 'spam', until: null })
    expect(fail).toHaveBeenCalledWith('Vous avez été banni de « Amis » : spam')
    expect(s.details.s1).toBeUndefined()
    fail.mockRestore()
  })
  it('suit l\'exclusion temporaire et le pseudo de serveur d\'un membre', () => {
    ev('member.timeout', { server_id: 's1', user_id: 'bob', until: '2099-01-01T00:00:00Z', reason: '' })
    expect(s.details.s1.members.find((m) => m.id === 'bob')?.timeout_until).toBe('2099-01-01T00:00:00Z')
    ev('member.timeout', { server_id: 's1', user_id: 'bob', until: null, reason: '' })
    expect(s.details.s1.members.find((m) => m.id === 'bob')?.timeout_until).toBeNull()
    ev('member.nickname', { server_id: 's1', user_id: 'bob', nickname: 'Bobby' })
    expect(s.details.s1.members.find((m) => m.id === 'bob')?.nickname).toBe('Bobby')
  })
  it('suit la session « Regarder ensemble » d\'un salon vocal avec le décalage d\'horloge', () => {
    const state = { rev: 1, video: 'dQw4w9WgXcQ', playing: true, pos: 0, rate: 1, at: '2026-10-28T10:00:00Z', host: 'me', host_only: false, by_id: 'me', by: 'moi', act: 'video', queue: [], ready: {} }
    ev('watch.state', { server_id: 's1', channel_id: 'v1', state, now: '2026-10-28T10:00:01Z' })
    expect(s.watch.v1.state.video).toBe('dQw4w9WgXcQ')
    expect(typeof s.watch.v1.offset).toBe('number')
    ev('watch.state', { server_id: 's1', channel_id: 'v1', state: null, now: '2026-10-28T10:00:02Z' })
    expect(s.watch.v1).toBeUndefined()
  })
  it('suit la partie Draw2Gether : état numéroté, mot privé, traits ordonnés et chat', () => {
    ;(api as unknown as Record<string, unknown>).draw = vi.fn(async () => ({ state: null, now: 0 }))
    const st = (rev: number, over: Record<string, unknown> = {}) => ({ rev, server_id: 's1', host: 'bob', phase: 'drawing', settings: {}, players: [], round: 1, drawer: 'bob', deadline: 90_000, mask: '____', word_len: 4, reveal: null, reason: '', draw_seq: 3, reports: 0, skip_votes: 0, winners: [], ...over })
    ev('draw.state', { server_id: 's1', channel_id: 'v1', state: st(2), now: Date.now() })
    expect(draw.games.v1.state.mask).toBe('____')
    ev('draw.state', { server_id: 's1', channel_id: 'v1', state: st(1, { mask: 'ancien' }), now: Date.now() })
    expect(draw.games.v1.state.mask).toBe('____')
    ev('draw.private', { server_id: 's1', channel_id: 'v1', rev: 2, word: 'chat' })
    expect(draw.games.v1.me.word).toBe('chat')
    const trait = { op: 'stroke', id: 'a', tool: 'pen', color: '#101010', size: 4, pts: [[0.1, 0.1]] }
    ev('draw.ops', { server_id: 's1', channel_id: 'v1', seq: 4, ops: [trait] })
    ev('draw.ops', { server_id: 's1', channel_id: 'v1', seq: 4, ops: [trait] })
    ev('draw.ops', { server_id: 's1', channel_id: 'v1', seq: 5, ops: [{ ...trait, pts: [[0.2, 0.2]] }] })
    expect(draw.games.v1.strokes[0].pts).toEqual([[0.1, 0.1], [0.2, 0.2]])
    expect(draw.games.v1.seq).toBe(5)
    ev('draw.ops', { server_id: 's1', channel_id: 'v1', seq: 6, ops: [{ op: 'undo' }] })
    expect(draw.games.v1.strokes).toEqual([])
    ev('draw.chat', { server_id: 's1', channel_id: 'v1', kind: 'guess', from: 'bob', pseudo: 'bob', text: 'chien', at: 1 })
    expect(draw.games.v1.chat).toHaveLength(1)
    ev('draw.state', { server_id: 's1', channel_id: 'v1', state: st(3, { phase: 'turn_end', reveal: 'chat' }), now: Date.now() })
    expect(draw.games.v1.me).toEqual({})
    ev('draw.state', { server_id: 's1', channel_id: 'v1', state: null, now: 0 })
    expect(draw.games.v1).toBeUndefined()
  })
  it('affiche le morceau écouté par les personnes d\'un salon vocal en session Music2gether', () => {
    const meta = { title: 'Titre', artist: 'Artiste', album: 'Album', duration: 180, cover: '/music/cover/dQw4w9WgXcQ' }
    const state = { rev: 1, video: 'dQw4w9WgXcQ', playing: true, pos: 0, rate: 1, at: '2026-10-28T10:00:00Z', host: 'me', host_only: false, by_id: 'me', by: 'moi', act: 'video', queue: [], ready: {}, meta, repeat: '', history: [] }
    s.voice.v9 = [{ user_id: 'bob', pseudo: 'Bob' }]
    expect(s.listeningOf('bob')).toBe('')
    ev('music.state', { server_id: 's1', channel_id: 'v9', state, now: '2026-10-28T10:00:01Z' })
    expect(s.listeningOf('bob')).toBe('Titre – Artiste')
    expect(s.listeningOf('alice')).toBe('')
    ev('music.state', { server_id: 's1', channel_id: 'v9', state: null, now: '2026-10-28T10:00:02Z' })
    expect(s.listeningOf('bob')).toBe('')
    expect(s.watch.v9).toBeUndefined() // séparée de « Regarder ensemble »
  })
  it('suit la séance d\'un salon cinéma, séparée de « Regarder ensemble »', () => {
    const state = (rev: number, playing: boolean) => ({ rev, provider: 'film', video: 'e1', playing, pos: 120, rate: 1, at: '2026-10-28T10:00:00Z', host: 'me', host_only: true, by_id: 'me', by: 'moi', act: 'seek', queue: [], ready: {}, meta: { title: 'Film', artist: '', album: '', duration: 3000, cover: '' }, repeat: '', history: [] })
    ev('cinema.state', { server_id: 's1', channel_id: 'c1', state: state(3, true), now: '2026-10-28T10:00:01Z' })
    expect(s.cinema.c1.state.pos).toBe(120)
    expect(s.watch.c1).toBeUndefined()
    ev('cinema.state', { server_id: 's1', channel_id: 'c1', state: state(2, false), now: '2026-10-28T10:00:02Z' })
    expect(s.cinema.c1.state.playing).toBe(true)
    ev('channel.deleted', { server_id: 's1', id: 'c1' })
    expect(s.cinema.c1).toBeUndefined()
    ev('cinema.state', { server_id: 's1', channel_id: 'c1', state: state(4, true), now: '2026-10-28T10:00:03Z' })
    ev('cinema.state', { server_id: 's1', channel_id: 'c1', state: null, now: '2026-10-28T10:00:04Z' })
    expect(s.cinema.c1).toBeUndefined()
  })
  it('un salon cinéma est un salon vocal', () => {
    expect(['text', 'voice', 'videos', 'music', 'cinema'].filter((k) => isVoiceKind(k as ChannelKind))).toEqual(['voice', 'cinema'])
  })
  it('ignore un état de « Regarder ensemble » plus ancien que celui déjà reçu', () => {
    const state = (rev: number, playing: boolean) => ({ rev, video: 'dQw4w9WgXcQ', playing, pos: 0, rate: 1, at: '2026-10-28T10:00:00Z', host: 'me', host_only: false, by_id: 'me', by: 'moi', act: 'pause', queue: [], ready: {} })
    ev('watch.state', { server_id: 's1', channel_id: 'v2', state: state(6, false), now: '2026-10-28T10:00:01Z' })
    ev('watch.state', { server_id: 's1', channel_id: 'v2', state: state(5, true), now: '2026-10-28T10:00:02Z' })
    expect(s.watch.v2.state.playing).toBe(false)
    delete s.watch.v2
  })
  it('recharge un salon « Vidéos » ouvert quand il change, et ignore les autres', async () => {
    const load = vi.mocked(api.videos)
    load.mockClear()
    ev('videos.changed', { server_id: 's1', channel_id: 'x1' })
    expect(load).not.toHaveBeenCalled()
    await videos.load('x1')
    load.mockClear()
    ev('videos.changed', { server_id: 's1', channel_id: 'x1' })
    expect(load).toHaveBeenCalledWith('x1')
    await videos.load('x1')
    ev('channel.deleted', { server_id: 's1', id: 'x1' })
    expect(videos.boards.x1).toBeUndefined()
  })
  it('retire les messages supprimés en masse', () => {
    ev('message.created', msg({ id: '1' }))
    ev('message.created', msg({ id: '2' }))
    ev('message.created', msg({ id: '3' }))
    ev('messages.deleted', { channel_id: 'c1', ids: ['1', '3'] })
    expect(s.convs['c:c1'].messages.map((m) => m.id)).toEqual(['2'])
  })
  it('rejoint le salon cible quand un modérateur déplace le membre, seulement s\'il y était', () => {
    const move = vi.spyOn(voice, 'move').mockResolvedValue()
    voice.channelId = 'v1'
    ev('voice.moved', { server_id: 's1', from: 'v1', to: 'v2' })
    expect(move).toHaveBeenCalledWith('v2')
    move.mockClear()
    voice.channelId = 'autre'
    ev('voice.moved', { server_id: 's1', from: 'v1', to: 'v2' })
    expect(move).not.toHaveBeenCalled()
    voice.channelId = null
    voice.dropped = { id: 'v1', at: Date.now() }
    ev('voice.moved', { server_id: 's1', from: 'v1', to: 'v2', by: 'Modo', to_name: 'Salon 2' })
    expect(move).toHaveBeenCalledWith('v2')
    voice.dropped = null
    voice.channelId = null
    move.mockRestore()
  })
  it('gère les salons', () => {
    ev('channel.created', { server_id: 's1', channel: { id: 'c9', name: 'nouveau', kind: 'text', position: 3 } })
    ev('channel.updated', { server_id: 's1', id: 'c9', name: 'renommé' })
    expect(s.details.s1.channels.find((c) => c.id === 'c9')?.name).toBe('renommé')
    ev('channel.deleted', { server_id: 's1', id: 'c9' })
    expect(s.details.s1.channels.some((c) => c.id === 'c9')).toBe(false)
  })
  it('place un salon créé après celui demandé et ignore un doublon', () => {
    ev('channel.created', { server_id: 's1', channel: { id: 'c2', name: 'deux', kind: 'text', position: 2 } })
    ev('channel.created', { server_id: 's1', channel: { id: 'c3', name: 'trois', kind: 'text', position: 3 }, after: 'c1' })
    ev('channel.created', { server_id: 's1', channel: { id: 'c3', name: 'trois', kind: 'text', position: 3 }, after: 'c1' })
    expect(s.details.s1.channels.map((c) => c.id)).toEqual(['c1', 'c3', 'c2'])
  })
  it('valide le nom d\'un salon en direct', () => {
    const existing = [{ name: 'Général', kind: 'text' as const }, { name: 'jeux', kind: 'voice' as const }]
    expect(channelNameError('', 'text', existing)).toMatch(/nom/)
    expect(channelNameError('   ', 'text', existing)).toMatch(/nom/)
    expect(channelNameError('x'.repeat(31), 'text', existing)).toMatch(/30/)
    expect(channelNameError('x'.repeat(30), 'text', existing)).toBe('')
    expect(channelNameError('a\u0007b', 'text', existing)).toMatch(/non autorisé/)
    expect(channelNameError(' général ', 'text', existing)).toMatch(/déjà/)
    expect(channelNameError('jeux', 'text', existing)).toBe('')
    expect(channelNameError('jeux', 'voice', existing)).toMatch(/déjà/)
  })
  it('réordonne les salons et suit la couleur de bannière', () => {
    ev('channel.created', { server_id: 's1', channel: { id: 'c8', name: 'huit', kind: 'text', position: 3 } })
    const texts = s.details.s1.channels.filter((c) => c.kind === 'text').map((c) => c.id).reverse()
    ev('channel.reordered', { server_id: 's1', kind: 'text', ids: texts })
    expect(s.details.s1.channels.filter((c) => c.kind === 'text').map((c) => c.id)).toEqual(texts)
    ev('banner.updated', { user_id: 'bob', color: '#aa00ff' })
    expect(s.details.s1.members.find((m) => m.id === 'bob')?.banner_color).toBe('#aa00ff')
  })
  it('suit les participants du vocal', () => {
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: true })
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: true })
    expect(s.voice.v1).toHaveLength(1)
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: false })
    expect(s.voice.v1).toHaveLength(0)
  })
  it('retient l\'heure d\'arrivée et en déduit la durée de présence du salon', () => {
    expect(oldestSince(s.voice.v1)).toBeNull()
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: true, at: '2026-10-21T10:00:10Z' })
    ev('voice.state', { channel_id: 'v1', user_id: 'me', pseudo: 'moi', joined: true, at: '2026-10-21T10:00:30Z' })
    expect(oldestSince(s.voice.v1)).toBe(Date.parse('2026-10-21T10:00:10Z'))
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: false })
    expect(oldestSince(s.voice.v1)).toBe(Date.parse('2026-10-21T10:00:30Z'))
    ev('voice.state', { channel_id: 'v1', user_id: 'me', pseudo: 'moi', joined: false })
    expect(oldestSince(s.voice.v1)).toBeNull()
    expect(oldestSince([{}])).toBeNull()
  })
  it('met en forme les durées', () => {
    expect([5, 65, 3725].map(formatDuration)).toEqual(['5 s', '1:05', '1 h 02'])
    expect([59 * 60, 3725 * 2].map(formatVoiceTotal)).toEqual(['59 min', '2 h 04'])
  })
  it('calcule le temps vocal du profil : cumulé, en cours, sans historique', () => {
    expect(voiceTimes(undefined, 0, null, 1000)).toEqual({ total: 0, current: null })
    expect(voiceTimes(600, 0, null, 99000)).toEqual({ total: 600, current: null })
    expect(voiceTimes(600, 10000, 5000, 70000)).toEqual({ total: 660, current: 65 })
    expect(voiceTimes(600, 0, 5000, 65000)).toEqual({ total: 660, current: 60 })
  })
  it('retrouve l\'arrivée d\'un membre dans le vocal d\'un serveur', () => {
    s.details.s1 = { server: { id: 's1' }, channels: [{ id: 'v1', kind: 'voice' }], members: [] } as never
    expect(s.voiceSinceOf('s1', 'bob')).toBeNull()
    ev('voice.state', { server_id: 's1', channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: true, at: '2026-10-21T10:00:10Z' })
    expect(s.voiceSinceOf('s1', 'bob')).toBe(Date.parse('2026-10-21T10:00:10Z'))
    expect(s.voiceSinceOf('s1', 'eve')).toBeNull()
  })
  it('suit les partages d\'écran et de caméra', () => {
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: true })
    ev('voice.share', { server_id: 's1', channel_id: 'v1', user_id: 'bob', pseudo: 'bob', source: 'screen', on: true })
    expect(s.voice.v1[0].screen).toBe(true)
    ev('voice.share', { server_id: 's1', channel_id: 'v1', user_id: 'bob', pseudo: 'bob', source: 'screen', on: false })
    expect(s.voice.v1[0].screen).toBe(false)
  })
  it("retire l'icône caméra à la coupure et la remet à la réactivation", () => {
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: true })
    ev('voice.share', { server_id: 's1', channel_id: 'v1', user_id: 'bob', pseudo: 'bob', source: 'camera', on: true })
    expect(s.voice.v1[0].camera).toBe(true)
    ev('voice.share', { server_id: 's1', channel_id: 'v1', user_id: 'bob', pseudo: 'bob', source: 'camera', on: false })
    expect(s.voice.v1[0].camera).toBe(false)
    ev('voice.share', { server_id: 's1', channel_id: 'v1', user_id: 'bob', pseudo: 'bob', source: 'camera', on: true })
    expect(s.voice.v1[0].camera).toBe(true)
  })
  it('affiche la coupure du micro ou du son posée par le créateur, pour tous', () => {
    ev('voice.state', { channel_id: 'v1', user_id: 'bob', pseudo: 'bob', joined: true })
    ev('voice.mod', { server_id: 's1', channel_id: 'v1', user_id: 'bob', mic_off: true, sound_off: false })
    expect(s.voice.v1[0]).toMatchObject({ mic_off: true, sound_off: false })
    ev('voice.mod', { server_id: 's1', channel_id: 'v1', user_id: 'bob', mic_off: false, sound_off: false })
    expect(s.voice.v1[0]).toMatchObject({ mic_off: false, sound_off: false })
  })
  it('supprime un serveur et ses conversations', () => {
    ev('server.deleted', { id: 's1' })
    expect(s.servers).toHaveLength(0)
    expect(s.convs['c:c1']).toBeUndefined()
  })
})

describe('mentions', () => {
  it('compte les mentions par conversation et les efface à l\'ouverture', async () => {
    ev('mention', msg({ id: '7', mentions: ['me'] }))
    ev('mention', msg({ id: '7', mentions: ['me'] })) // doublon ignoré
    expect(s.mentionCount('c:c1')).toBe(1)
    expect(s.serverMentionCount('s1')).toBe(1)
    expect(s.totalMentions).toBe(1)
    await s.open('c:c1')
    expect(s.mentionCount('c:c1')).toBe(0)
  })
  it('ne compte pas une mention de groupe quand le serveur est ignoré', () => {
    ev('mention', { ...msg({ id: '8', mentions: ['me'] }), group_mention: true })
    expect(s.mentionCount('c:c1')).toBe(1)
    s.notifySettings.ignore_group = ['s1']
    expect(s.mentionCount('c:c1')).toBe(0)
    expect(s.serverMentionCount('s1')).toBe(0)
    expect(s.totalMentions).toBe(0)
    s.notifySettings.ignore_group = []
  })
  it('liste les pseudos d\'un salon pour l\'autocomplétion', () => {
    expect(s.pseudosOf('c:c1').sort()).toEqual(['bob', 'moi'])
    expect(s.pseudosOf(null)).toEqual([])
  })
})

describe('photo de profil par serveur', () => {
  it('suit la photo propre au serveur sans toucher aux autres serveurs', () => {
    ev('server_avatar.updated', { server_id: 's1', user_id: 'bob', version: 2 })
    expect(avatars.serverVersion('bob', 's1')).toBe(2)
    expect(avatars.serverVersion('bob', 's2')).toBe(0)
    expect(avatars.serverVersion('bob')).toBe(0)
  })
  it('retombe sur la photo globale quand la photo du serveur est retirée', () => {
    ev('avatar.updated', { user_id: 'carl', version: 1 })
    ev('server_avatar.updated', { server_id: 's1', user_id: 'carl', version: 1 })
    ev('server_avatar.updated', { server_id: 's1', user_id: 'carl', version: 0 })
    expect(avatars.serverVersion('carl', 's1')).toBe(0)
    expect(avatars.src('carl', 's1')).toBe(avatars.src('carl'))
  })
})

describe('compte bot', () => {
  it('repère le compte Estompe par son identifiant, pas par son pseudo', () => {
    expect(isBot(BOT_ID)).toBe(true)
    expect(isBot('u1')).toBe(false)
  })
})

describe('icône de serveur', () => {
  it('suit la version de l\'icône dans la liste et le détail, et la retire à 0', () => {
    ev('server_icon.updated', { id: 's1', version: 3 })
    expect(s.servers[0].icon_version).toBe(3)
    expect(s.details.s1.server.icon_version).toBe(3)
    ev('server_icon.updated', { id: 's1', version: 0 })
    expect(s.servers[0].icon_version).toBe(0)
  })
})

describe('rôles et permissions', () => {
  const role = (o: object) => ({ id: 'r1', name: 'Modérateur', color: '', icon: '', position: 1, hoist: false, permissions: [], is_default: false, ...o })
  it('lit les permissions du salon, à défaut celles du serveur', () => {
    s.details.s1.permissions = ['send_messages', 'create_invites']
    s.details.s1.channels[0].permissions = ['send_messages']
    expect(s.can('s1', 'send_messages', 'c1')).toBe(true)
    expect(s.can('s1', 'create_invites', 'c1')).toBe(false)
    expect(s.can('s1', 'create_invites')).toBe(true)
    expect(s.canConv('c:c1', 'create_polls')).toBe(false)
    expect(s.canConv('d:x', 'create_polls')).toBe(true)
  })
  it('ne propose les mentions de groupe qu\'avec la permission', () => {
    s.details.s1.roles = [role({}), role({ id: 'e', name: '@tout le monde', is_default: true })]
    s.details.s1.channels[0].permissions = []
    expect(s.groupMentionsOf('c:c1')).toEqual(['everyone', 'here', 'Modérateur'])
    expect(s.groupSuggestionsOf('c:c1')).toEqual([])
    s.details.s1.channels[0].permissions = ['mention_everyone']
    expect(s.groupSuggestionsOf('c:c1')).toEqual(['everyone', 'here', 'Modérateur'])
  })
  it('recharge le serveur quand ses rôles changent, sans reconnexion', async () => {
    ev('roles.updated', { server_id: 's1' })
    await vi.waitFor(() => expect(s.details.s1.permissions).toEqual(['view_channel']))
    expect(s.details.s1.channels[0].permissions).toEqual(['view_channel'])
  })
})
