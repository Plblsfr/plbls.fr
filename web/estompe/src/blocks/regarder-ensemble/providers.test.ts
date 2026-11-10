import { afterEach, describe, expect, it, vi } from 'vitest'
import { providers, readVimeo, readYouTube, twitchSrc, type PState } from './providers'

const recu = (f: HTMLIFrameElement, data: unknown, origin: string, source: Window | null = f.contentWindow) =>
  dispatchEvent(new MessageEvent('message', { data, origin, source }))

async function lecteur(nom: string) {
  const el = document.body.appendChild(document.createElement('div'))
  const etats: PState[] = []
  const erreurs: string[] = []
  const a = await providers[nom](el, { onState: (s) => etats.push(s), onError: (m) => erreurs.push(m) })
  return { el, a, etats, erreurs }
}
const envoyes = (spy: ReturnType<typeof vi.fn>) => spy.mock.calls.map(([m, o]) => [JSON.parse(m as string), o])

afterEach(() => { document.body.replaceChildren(); vi.restoreAllMocks() })

describe('messages des lecteurs intégrés', () => {
  it('YouTube : prêt, état, position, durée, vitesse et erreurs', () => {
    expect(readYouTube('{"event":"onReady","id":1,"channel":"widget"}')).toEqual({ ready: true })
    expect(readYouTube({ event: 'onStateChange', info: 1 })).toEqual({ state: 'playing' })
    expect(readYouTube({ event: 'infoDelivery', info: { currentTime: 12.5, duration: 90, playbackRate: 1.25, playerState: 2 } })).toEqual({ time: 12.5, duration: 90, rate: 1.25, state: 'paused' })
    expect(readYouTube({ event: 'infoDelivery', info: { currentTime: 'x', videoData: {} } })).toEqual({})
    expect(readYouTube({ event: 'onError', info: 150 })).toEqual({ error: 150 })
    expect(readYouTube({ event: 'onAutoplayBlocked' })).toEqual({ blocked: true })
    for (const x of ['pas du json', '[1]', null, 42, { event: 'inconnu' }, { event: 'onStateChange', info: '1' }]) expect(readYouTube(x)).toBeNull()
  })

  it('Vimeo : évènements, position et réponses aux méthodes', () => {
    expect(readVimeo('{"event":"ready","player_id":"p"}')).toEqual({ event: 'ready' })
    expect(readVimeo({ event: 'timeupdate', data: { seconds: 3.2, percent: 0.1, duration: 32 } })).toEqual({ event: 'timeupdate', time: 3.2, duration: 32 })
    expect(readVimeo({ event: 'error', data: { method: 'play', name: 'NotAllowedError' } })).toEqual({ event: 'error', method: 'play' })
    expect(readVimeo({ method: 'getDuration', value: 61 })).toEqual({ event: 'duration', duration: 61 })
    for (const x of ['{', 7, { method: 'getDuration', value: 'x' }, { autre: 1 }]) expect(readVimeo(x)).toBeNull()
  })

  it('Twitch : adresse de l’iframe avec le domaine parent', () => {
    expect(twitchSrc('vod', '123', 3723, true, 'estompe.test')).toBe('https://player.twitch.tv/?video=v123&time=1h2m3s&parent=estompe.test&autoplay=true')
    expect(twitchSrc('live', 'chaine', 0, false, 'tauri.localhost')).toBe('https://player.twitch.tv/?channel=chaine&parent=tauri.localhost&autoplay=false')
    expect(twitchSrc('clip', 'Drole Clip', 0, true, 'estompe.test')).toBe('https://clips.twitch.tv/embed?clip=Drole%20Clip&parent=estompe.test&autoplay=true')
  })
})

describe('lecteurs pilotés par postMessage', () => {
  it('YouTube : aucun script, commandes gardées jusqu’à onReady, messages d’une autre origine ou fenêtre ignorés', async () => {
    const { el, a, etats, erreurs } = await lecteur('youtube')
    expect(document.querySelector('script')).toBeNull()
    a.load('dQw4w9WgXcQ', 30, true)
    const f = el.querySelector('iframe')!
    const src = new URL(f.src)
    expect([src.origin, src.pathname, src.searchParams.get('enablejsapi'), src.searchParams.get('start'), src.searchParams.get('autoplay'), src.searchParams.get('origin')]).toEqual(['https://www.youtube-nocookie.com', '/embed/dQw4w9WgXcQ', '1', '30', '1', location.origin])
    expect(f.referrerPolicy).toBe('strict-origin-when-cross-origin')
    const post = vi.spyOn(f.contentWindow!, 'postMessage').mockImplementation(() => {})
    f.dispatchEvent(new Event('load'))
    a.pause()
    expect(envoyes(post).map(([m]) => m.event)).toEqual(['listening'])
    recu(f, { event: 'onReady' }, 'https://evil.test')
    recu(f, { event: 'onReady' }, 'https://www.youtube-nocookie.com', window)
    recu(f, { event: 'onError', info: 100 }, 'https://www.youtube.com')
    expect(post).toHaveBeenCalledTimes(1)
    recu(f, '{"event":"onReady"}', 'https://www.youtube-nocookie.com')
    a.seek(42)
    expect(envoyes(post).slice(1)).toEqual([
      [expect.objectContaining({ event: 'command', func: 'pauseVideo', args: [] }), 'https://www.youtube-nocookie.com'],
      [expect.objectContaining({ event: 'command', func: 'seekTo', args: [42, true] }), 'https://www.youtube-nocookie.com'],
    ])
    recu(f, { event: 'infoDelivery', info: { currentTime: 43, duration: 200, playerState: 1 } }, 'https://www.youtube-nocookie.com')
    recu(f, { event: 'infoDelivery', info: { currentTime: 44, playerState: 1 } }, 'https://www.youtube-nocookie.com')
    expect([a.time(), a.duration?.(), a.state(), etats, erreurs]).toEqual([44, 200, 'playing', ['playing'], []])
    a.load('aaaaaaaaaaa', 5, false)
    expect(envoyes(post).at(-1)![0]).toMatchObject({ func: 'cueVideoById', args: [{ videoId: 'aaaaaaaaaaa', startSeconds: 5 }] })
    recu(f, { event: 'onError', info: 150 }, 'https://www.youtube-nocookie.com')
    expect(erreurs).toHaveLength(1)
    a.destroy()
    expect(el.querySelector('iframe')).toBeNull()
  })

  it('Vimeo : abonnements et position de départ à « ready », lecture refusée sans erreur', async () => {
    const { el, a, etats, erreurs } = await lecteur('vimeo')
    a.load('76979871', 12, true)
    const f = el.querySelector('iframe')!
    expect(f.src).toBe('https://player.vimeo.com/video/76979871?api=1&autoplay=0&playsinline=1&dnt=1')
    const post = vi.spyOn(f.contentWindow!, 'postMessage').mockImplementation(() => {})
    recu(f, { event: 'ready' }, 'https://vimeo.com')
    expect(post).not.toHaveBeenCalled()
    recu(f, '{"event":"ready"}', 'https://player.vimeo.com')
    const m = envoyes(post).map(([x]) => x)
    expect(m.filter((x) => x.method === 'addEventListener').map((x) => x.value)).toEqual(expect.arrayContaining(['timeupdate', 'play', 'pause', 'ended']))
    expect(m.slice(-2)).toEqual([{ method: 'setCurrentTime', value: 12 }, { method: 'play' }])
    recu(f, { event: 'timeupdate', data: { seconds: 13, duration: 100 } }, 'https://player.vimeo.com')
    recu(f, { event: 'playing' }, 'https://player.vimeo.com')
    recu(f, { event: 'error', data: { method: 'play' } }, 'https://player.vimeo.com')
    expect([a.time(), a.duration?.(), etats, erreurs]).toEqual([13, 100, ['cued', 'playing', 'paused'], []])
    a.seek(50)
    expect(envoyes(post).at(-1)![0]).toEqual({ method: 'setCurrentTime', value: 50 })
    recu(f, { event: 'error', data: { name: 'PrivacyError' } }, 'https://player.vimeo.com')
    expect(erreurs).toHaveLength(1)
  })

  it('Twitch : iframe seule, lancement synchronisé seulement', async () => {
    const { el, a, etats } = await lecteur('twitch_vod')
    expect(a.sync).toBe('start')
    a.load('123', 60, false)
    const f = el.querySelector('iframe')!
    expect(f.src).toContain('https://player.twitch.tv/?video=v123&time=0h1m0s')
    f.dispatchEvent(new Event('load'))
    expect(etats).toEqual(['cued'])
    expect(document.querySelector('script')).toBeNull()
  })
})
