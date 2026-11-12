import { describe, expect, it, vi } from 'vitest'
import { msgOverlaySize, msgToastView, msgToastWanted, MSG_TOAST_SIZE, overlayAccelerator, overlayItems, overlayPlacement, overlayPosition, overlayVisible, overlaySize, pushMsgToast, sendMsgReply, type MsgToast, type OverlayItem } from './overlay'
import { avatars } from '../../../../web/estompe/src/blocks/avatars/avatars.svelte'

vi.mock('../../../../web/estompe/src/lib/api', () => ({
  api: {
    avatars: vi.fn(async () => ({ u1: 3 })),
    serverAvatars: vi.fn(),
    avatarBlob: vi.fn(async () => new Blob(['img'], { type: 'image/webp' })),
  },
}))

const it1 = (id: string, speaking: boolean): OverlayItem => ({ id, pseudo: id, avatar: '', speaking, muted: false, deaf: false })

describe('overlay', () => {
  it('n\'affiche que ceux qui parlent en mode « parlent »', () => {
    const l = [it1('a', true), it1('b', false)]
    expect(overlayItems(l, 'all')).toHaveLength(2)
    expect(overlayItems(l, 'speaking').map((i) => i.id)).toEqual(['a'])
  })
  it('place la fenêtre dans le coin choisi, en pixels logiques', () => {
    const mon = { x: 1920, y: 0, w: 3840, h: 2160, sf: 2 }
    const win = overlaySize('md', 2)
    expect(overlayPosition('tl', mon, win)).toEqual({ x: 976, y: 16 })
    const br = overlayPosition('br', mon, win)
    expect(br.x).toBe(960 + 1920 - win.w - 16)
    expect(br.y).toBe(1080 - win.h - 16)
  })
  it('donne à l’overlay natif sa place et sa taille en pixels physiques', () => {
    const mon = { x: 1920, y: 0, w: 3840, h: 2160, sf: 1.25 }
    const win = overlaySize('md', 2)
    expect(overlayPlacement(overlayPosition('tl', mon, win), win, 1.25)).toEqual({ x: 1940, y: 20, largeur: 325, hauteur: 120, echelle: 1.25 })
  })
  it('centre la fenêtre verticalement au milieu de l\'écran', () => {
    const mon = { x: 0, y: 0, w: 1920, h: 1080, sf: 1 }
    const win = { w: 100, h: 200 }
    expect(overlayPosition('ml', mon, win)).toEqual({ x: 16, y: 440 })
    expect(overlayPosition('mr', mon, win)).toEqual({ x: 1804, y: 440 })
  })
  it('affiche selon le mode, le jeu au premier plan et le masquage', () => {
    expect(overlayVisible('games', true, false)).toBe(true)
    expect(overlayVisible('games', false, false)).toBe(false)
    expect(overlayVisible('always', false, false)).toBe(true)
    expect(overlayVisible('never', true, false)).toBe(false)
    expect(overlayVisible('always', true, true)).toBe(false)
  })
  it('construit le raccourci global depuis les modificateurs', () => {
    const e = { code: 'KeyO', ctrlKey: true, altKey: false, shiftKey: true, metaKey: false }
    expect(overlayAccelerator(e)).toBe('Control+Shift+KeyO')
    expect(overlayAccelerator({ ...e, code: 'ShiftLeft' })).toBe('')
  })
})

describe('photos de profil de l\'overlay', () => {
  it('garde une image exploitable pour un participant qui a une photo', async () => {
    URL.createObjectURL = vi.fn(() => 'blob:avatar-u1')
    await avatars.load()
    avatars.ensure('u1')
    await vi.waitFor(() => expect(avatars.src('u1')).toBe('blob:avatar-u1'))
    expect(avatars.blob(avatars.src('u1'))?.type).toBe('image/webp')
  })
  it('ne fournit rien pour un participant sans photo', () => {
    expect(avatars.src('sans-photo')).toBe('')
    expect(avatars.blob('')).toBeUndefined()
  })
})

describe('overlay des messages', () => {
  const toast = (id: string, conv = 'd:1'): MsgToast => ({ id, conv, href: '#/dm/1', title: 'Ana', body: 'salut', avatar: '', author: 'u1' })
  it('filtre selon MP, mentions ou les deux', () => {
    expect(msgToastWanted('dm', true, 'both')).toBe(true)
    expect(msgToastWanted('mention', true, 'dm')).toBe(false)
    expect(msgToastWanted('dm', true, 'mention')).toBe(false)
    expect(msgToastWanted('mention', true, 'mention')).toBe(true)
    expect(msgToastWanted('channel', true, 'both')).toBe(false)
    expect(msgToastWanted('voice', false, 'both')).toBe(false)
    expect(msgToastWanted('dm', true, 'off')).toBe(false)
  })
  it('limite la pile et remplace un message déjà affiché', () => {
    let l: MsgToast[] = []
    for (const id of ['1', '2', '3', '4', '5']) l = pushMsgToast(l, toast(id))
    expect(l.map((t) => t.id)).toEqual(['2', '3', '4', '5'])
    expect(pushMsgToast(l, toast('3')).map((t) => t.id)).toEqual(['2', '4', '5', '3'])
  })
  it('masque le contenu à la demande', () => {
    expect(msgToastView(toast('1'), true).body).toBe('')
    expect(msgToastView(toast('1'), false).body).toBe('salut')
  })
  it('agrandit la fenêtre pour le champ de réponse', () => {
    expect(msgOverlaySize(2, true).h).toBe(msgOverlaySize(2, false).h + MSG_TOAST_SIZE.reply)
  })
  it('répond au dernier message, le marque lu et retire sa conversation', async () => {
    const send = vi.fn(async () => {})
    const read = vi.fn()
    const l = [toast('1', 'd:1'), toast('2', 'd:2'), toast('3', 'd:1')]
    expect((await sendMsgReply(l, '  ok  ', send, read)).map((t) => t.id)).toEqual(['2'])
    expect(send).toHaveBeenCalledWith('d:1', 'ok')
    expect(read).toHaveBeenCalledWith('d:1')
  })
  it('n\'envoie rien si la réponse est vide ou sans notification', async () => {
    const send = vi.fn()
    expect(await sendMsgReply([toast('1')], '   ', send, vi.fn())).toHaveLength(1)
    expect(await sendMsgReply([], 'x', send, vi.fn())).toEqual([])
    expect(send).not.toHaveBeenCalled()
  })
})
