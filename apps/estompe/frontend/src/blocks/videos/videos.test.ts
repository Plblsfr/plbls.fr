import { describe, expect, it, vi } from 'vitest'
import { artistOf, countRsvp, filterPosts, isTrackRef, musicLink, shuffled, listPosts, localInput, moved, NO_FILTER, playLink, videoTitle, sessionPhase, sourceOf, when } from './videos'
import { solo, trackOfPost } from './solo.svelte'
import type { VideoList, VideoPost } from '../../lib/types'

const post = (o: Partial<VideoPost>): VideoPost => ({
  message_id: '1', author: { id: 'a', pseudo: 'a' }, created_at: '2026-10-28T10:00:00Z', provider: 'youtube', ref: 'x', url: 'https://youtu.be/x',
  title: '', site: '', has_thumb: false, watched_at: null, reactions: 0, liked: false, replies: 0, ...o,
})
const posts = [
  post({ message_id: '10', provider: 'youtube', reactions: 1 }),
  post({ message_id: '9', provider: 'twitch_vod', author: { id: 'b', pseudo: 'b' }, reactions: 5, watched_at: '2026-10-28T11:00:00Z' }),
  post({ message_id: '100', provider: 'page', ref: 'https://x.fr/a.mp4', url: 'https://x.fr/page' }),
  post({ message_id: '8', provider: 'simple', reactions: 5 }),
]

describe('filtres et tri des cartes', () => {
  const ids = (l: VideoPost[]) => l.map((p) => p.message_id)
  it('trie par récence numérique puis par réactions', () => {
    expect(ids(filterPosts(posts, NO_FILTER, 'recent'))).toEqual(['100', '10', '9', '8'])
    expect(ids(filterPosts(posts, NO_FILTER, 'reactions'))).toEqual(['9', '8', '10', '100'])
  })
  it('filtre par source (les formes de Twitch et « page » sont regroupées), vue ou non, auteur et liste', () => {
    expect(ids(filterPosts(posts, { ...NO_FILTER, source: 'twitch' }, 'recent'))).toEqual(['9'])
    expect(ids(filterPosts(posts, { ...NO_FILTER, source: 'file' }, 'recent'))).toEqual(['100'])
    expect(ids(filterPosts(posts, { ...NO_FILTER, watched: 'seen' }, 'recent'))).toEqual(['9'])
    expect(ids(filterPosts(posts, { ...NO_FILTER, watched: 'unseen' }, 'recent'))).toEqual(['100', '10', '8'])
    expect(ids(filterPosts(posts, { ...NO_FILTER, author: 'b' }, 'recent'))).toEqual(['9'])
    const lists: VideoList[] = [{ id: 'l1', name: 'x', created_by: 'a', collaborative: true, items: ['8', '10'] }]
    expect(ids(filterPosts(posts, { ...NO_FILTER, list: 'l1' }, 'recent', lists))).toEqual(['10', '8'])
  })
  it('regroupe les sources et choisit le lien à lire', () => {
    expect(sourceOf('twitch_clip')).toBe('twitch')
    expect(sourceOf('vimeo')).toBe('vimeo')
    expect(playLink(posts[2])).toBe('https://x.fr/a.mp4')
    expect(playLink(posts[0])).toBe('https://youtu.be/x')
  })
})

describe('listes', () => {
  it('suit l\'ordre de la liste et ignore les cartes supprimées', () => {
    expect(listPosts({ id: 'l', name: 'x', created_by: 'a', collaborative: true, items: ['8', '404', '10'] }, posts).map((p) => p.message_id)).toEqual(['8', '10'])
  })
  it('déplace un élément sans sortir des bornes', () => {
    expect(moved(['a', 'b', 'c'], 1, -1)).toEqual(['b', 'a', 'c'])
    expect(moved(['a', 'b', 'c'], 2, 1)).toEqual(['a', 'b', 'c'])
    expect(moved(['a', 'b', 'c'], 0, 1)).toEqual(['b', 'a', 'c'])
  })
})

describe('séances', () => {
  const now = Date.parse('2026-10-28T10:00:00Z')
  it('compte les réponses', () => {
    expect(countRsvp({ rsvps: { a: 'yes', b: 'maybe', c: 'yes', d: 'no' } })).toEqual({ yes: 2, maybe: 1, no: 1 })
  })
  it('distingue séance à venir et en cours', () => {
    expect(sessionPhase({ starts_at: '2026-10-28T10:05:00Z', started_at: null }, now)).toBe('upcoming')
    expect(sessionPhase({ starts_at: '2026-10-28T09:59:00Z', started_at: null }, now)).toBe('live')
    expect(sessionPhase({ starts_at: '2026-10-28T10:05:00Z', started_at: '2026-10-28T10:00:00Z' }, now)).toBe('live')
  })
  it('décrit l\'échéance', () => {
    expect(when('2026-10-28T10:12:00Z', now)).toBe('dans 12 min')
    expect(when('2026-10-28T09:00:00Z', now)).toBe('maintenant')
    expect(when('2026-10-29T10:00:00Z', now)).toMatch(/^demain /)
  })
  it('formate une valeur datetime-local en heure locale', () => {
    expect(localInput(new Date(2026, 9, 28, 20, 30))).toBe('2026-10-28T20:30')
  })
})

describe('videoTitle', () => {
  it('ne montre jamais le lien brut', () => {
    const base = { provider: 'youtube', url: 'https://youtu.be/dQw4w9WgXcQ' }
    expect(videoTitle({ ...base, title: 'Ma vidéo' })).toBe('Ma vidéo')
    expect(videoTitle({ ...base, title: '' })).toBe('Vidéo YouTube')
    expect(videoTitle({ ...base, title: base.url })).toBe('Vidéo YouTube')
    expect(videoTitle({ provider: 'simple', url: 'https://x.fr/a', title: '' })).toBe('Lien vidéo')
  })
})

describe('salon Musique', () => {
  const tune = (o: Partial<VideoPost>) => post({ provider: 'ytmusic', ref: 'dQw4w9WgXcQ', ...o })
  it('lit l\'artiste de la carte, pas le nombre de titres d\'une playlist', () => {
    expect(artistOf({ site: 'Daft Punk · Discovery · 3:25' })).toBe('Daft Punk')
    expect(artistOf({ site: '12 titres' })).toBe('')
    expect(artistOf({ site: '' })).toBe('')
  })
  it('filtre par artiste et trie par titre', () => {
    const l = [tune({ message_id: '1', site: 'B · x', title: 'Zèbre' }), tune({ message_id: '2', site: 'A · x', title: 'Abeille' }), tune({ message_id: '3', site: 'A · y', title: 'Moto' })]
    expect(filterPosts(l, { ...NO_FILTER, artist: 'A' }, 'recent').map((p) => p.message_id)).toEqual(['3', '2'])
    expect(filterPosts(l, NO_FILTER, 'title').map((p) => p.title)).toEqual(['Abeille', 'Moto', 'Zèbre'])
  })
  it('distingue un morceau d\'une playlist dans le lien à lire', () => {
    expect(isTrackRef('dQw4w9WgXcQ')).toBe(true)
    expect(musicLink('dQw4w9WgXcQ')).toBe('https://music.youtube.com/watch?v=dQw4w9WgXcQ')
    expect(musicLink('OLAK5uy_abcdefghij')).toBe('https://music.youtube.com/playlist?list=OLAK5uy_abcdefghij')
  })
  it('mélange sans perdre ni dupliquer de morceau', () => {
    const a = [1, 2, 3, 4, 5, 6]
    const s = shuffled(a, () => 0)
    expect([...s].sort()).toEqual(a)
    expect(a).toEqual([1, 2, 3, 4, 5, 6])
  })
  it('écoute seul : la file avance puis s\'arrête', async () => {
    const posts = [tune({ message_id: '1', ref: 'aaaaaaaaaaa', title: 'Un', site: 'Art · x' }), tune({ message_id: '2', ref: 'bbbbbbbbbbb', title: 'Deux' })]
    expect(trackOfPost(posts[0])).toMatchObject({ id: 'aaaaaaaaaaa', title: 'Un', artist: 'Art', cover: '/music/cover/aaaaaaaaaaa' })
    await solo.play(posts)
    expect(solo.current?.id).toBe('aaaaaaaaaaa')
    expect(solo.playing).toBe(true)
    await solo.play([tune({ ref: 'ccccccccccc' })], { append: true })
    expect(solo.upcoming.map((t) => t.id)).toEqual(['bbbbbbbbbbb', 'ccccccccccc'])
    expect(solo.index).toBe(0)
    expect(solo.playing).toBe(true)
    solo.toggle()
    expect(solo.playing).toBe(false)
    solo.next(); solo.next()
    expect(solo.current?.id).toBe('ccccccccccc')
    solo.next()
    expect(solo.current).toBeUndefined()
    expect(solo.playing).toBe(false)
  })
  it('seek déplace le lecteur et la position affichée', () => {
    const seekTo = vi.fn()
    solo.seekTo = seekTo
    solo.seek(42)
    expect(seekTo).toHaveBeenCalledWith(42)
    expect(solo.position).toBe(42)
    solo.seekTo = null
  })
})
