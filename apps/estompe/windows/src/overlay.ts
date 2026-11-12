/** Overlay des participants du salon vocal (app Windows) : calculs purs, la fenêtre est gérée par overlay.svelte.ts. */
import type { MsgOverlayKinds, OverlayCorner, OverlayMode, OverlaySize, OverlayWhen } from '../../../../web/estompe/src/lib/prefs.svelte'
export type { MsgOverlayKinds, OverlayCorner, OverlayMode, OverlaySize, OverlayWhen }
export const OVERLAY_POS: Record<OverlayCorner, string> = { tl: 'Haut gauche', ml: 'Centre gauche', bl: 'Bas gauche', tr: 'Haut droite', mr: 'Centre droit', br: 'Bas droite' }

export interface OverlayItem { id: string; pseudo: string; avatar: string; speaking: boolean; muted: boolean; deaf: boolean }
export interface OverlayState { size: OverlaySize; items: OverlayItem[] }

export const OVERLAY_AVATAR: Record<OverlaySize, number> = { sm: 28, md: 40, lg: 56 }
const GAP = 8
const MARGIN = 16

/** Participants à afficher : tous, ou seulement ceux qui parlent. */
export function overlayItems(items: OverlayItem[], mode: OverlayMode): OverlayItem[] {
  return mode === 'speaking' ? items.filter((i) => i.speaking) : items
}

/** Faut-il afficher l'overlay ? `game` : une application plein écran sans bordure est au premier plan ; `hidden` : masqué par l'utilisateur. */
export function overlayVisible(when: OverlayWhen, game: boolean, hidden: boolean): boolean {
  return !hidden && (when === 'always' || (when === 'games' && game))
}

/** Raccourci global (format du plugin) à partir d'un évènement clavier ; '' tant qu'aucune touche autre qu'un modificateur n'est enfoncée. */
export function overlayAccelerator(e: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string {
  if (/^(Control|Alt|Shift|Meta|OS)(Left|Right)$/.test(e.code)) return ''
  return [e.ctrlKey && 'Control', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super', e.code].filter(Boolean).join('+')
}

/** Taille logique de la fenêtre pour `n` lignes. */
export function overlaySize(size: OverlaySize, n: number) {
  const av = OVERLAY_AVATAR[size]
  return { w: av + 220, h: Math.max(1, n * (av + GAP)) }
}

/** Position logique de la fenêtre au bord d'un écran (coordonnées physiques, comme Tauri les donne). */
export function overlayPosition(corner: OverlayCorner, mon: { x: number; y: number; w: number; h: number; sf: number }, win: { w: number; h: number }) {
  const left = mon.x / mon.sf + MARGIN
  const top = mon.y / mon.sf + MARGIN
  const right = (mon.x + mon.w) / mon.sf - win.w - MARGIN
  const bottom = (mon.y + mon.h) / mon.sf - win.h - MARGIN
  const middle = (mon.y + mon.h / 2) / mon.sf - win.h / 2
  return { x: corner.endsWith('l') ? left : right, y: corner.startsWith('t') ? top : corner.startsWith('m') ? middle : bottom }
}

export function overlayPlacement(pos: { x: number; y: number }, win: { w: number; h: number }, sf: number) {
  return { x: Math.round(pos.x * sf), y: Math.round(pos.y * sf), largeur: Math.round(win.w * sf), hauteur: Math.round(win.h * sf), echelle: sf }
}

export const MSG_TOAST_MAX = 4
export const MSG_TOAST_SIZE = { w: 340, row: 64, gap: 8, reply: 44 }

export interface MsgToast { id: string; conv: string; href: string; title: string; body: string; avatar: string; author: string }
export interface MsgOverlayState { toasts: MsgToast[]; replying: boolean }

/** Notification du serveur (déjà filtrée par ses réglages : niveaux, types, ne pas déranger) à montrer selon le choix « MP, mentions ou les deux ». */
export function msgToastWanted(kind: string, hasMsg: boolean, pref: MsgOverlayKinds): boolean {
  if (!hasMsg || pref === 'off') return false
  return (kind === 'dm' && pref !== 'mention') || (kind === 'mention' && pref !== 'dm')
}

/** Ajoute (ou remplace, même message) une notification ; au-delà de `max`, les plus anciennes sortent. */
export function pushMsgToast(list: MsgToast[], t: MsgToast, max = MSG_TOAST_MAX): MsgToast[] {
  return [...list.filter((x) => x.id !== t.id), t].slice(-max)
}

/** Contenu affiché : « masquer le contenu » ne garde que l'expéditeur. */
export function msgToastView(t: MsgToast, hideContent: boolean): MsgToast {
  return hideContent ? { ...t, body: '' } : t
}

/** Taille logique de la fenêtre pour `n` notifications, avec ou sans champ de réponse. */
export function msgOverlaySize(n: number, replying: boolean) {
  const { w, row, gap, reply } = MSG_TOAST_SIZE
  return { w, h: Math.max(1, n * (row + gap) + (replying ? reply : 0)) }
}

/** Envoie la réponse au dernier message affiché, le marque lu, et renvoie les notifications restantes (celles des autres conversations). */
export async function sendMsgReply(list: MsgToast[], text: string, send: (conv: string, text: string) => Promise<unknown>, read: (conv: string) => void): Promise<MsgToast[]> {
  const last = list[list.length - 1]
  const content = text.trim()
  if (!last || !content) return list
  await send(last.conv, content)
  read(last.conv)
  return list.filter((t) => t.conv !== last.conv)
}
