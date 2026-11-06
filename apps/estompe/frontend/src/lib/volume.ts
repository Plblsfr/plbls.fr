import type { MenuItem } from './ctxmenu.svelte'

/** Réglage d'écoute d'un participant pour une piste : volume en % (100 par défaut) et sourdine propre à cette piste. */
export interface ChannelPrefs { volumes: Record<string, number>; muted: Record<string, boolean> }

/** Utilisateur derrière une identité LiveKit : le partage natif est publié par `<id>#screen`, dont le volume est réglé sous `<id>`. */
export const ownerOf = (identity: string) => identity.replace(/#screen$/, '')

/** Gain (0 à 3) appliqué à la piste `ch` de `userId` ; le mode sourdine casque coupe tout. */
export function gainOf(deafened: boolean, p: ChannelPrefs, userId: string): number {
  return deafened || p.muted[userId] ? 0 : (p.volumes[userId] ?? 100) / 100
}

/**
 * Menu d'un participant vocal : muet et volume pour soi (rien sur soi-même) ; coupures du serveur pour qui en a la permission
 * (`canMute`, `canDeafen`) sur une cible située sous son rôle (`canModerate` tient compte de la cible).
 */
export function participantMenu(o: {
  self: boolean; muted: boolean; canModerate: boolean; canMute?: boolean; canDeafen?: boolean; mod: { mic_off?: boolean; sound_off?: boolean }
  setMuted: (muted: boolean) => void; adjust?: () => void; moderate: (mod: { mic_off: boolean; sound_off: boolean }) => void
}): MenuItem[] {
  if (o.self) return []
  const items: MenuItem[] = [{ label: o.muted ? 'Ne plus rendre muet pour moi' : 'Rendre muet pour moi', run: () => o.setMuted(!o.muted) }]
  if (o.adjust) items.push({ label: 'Régler le volume', run: o.adjust })
  if (o.canModerate) {
    const mic = !!o.mod.mic_off, son = !!o.mod.sound_off
    if (o.canMute ?? true) items.push({ label: mic ? 'Rétablir le micro du serveur' : 'Couper le micro du serveur', run: () => o.moderate({ mic_off: !mic, sound_off: son }) })
    if (o.canDeafen ?? true) items.push({ label: son ? 'Rétablir le son du serveur' : 'Couper le son du serveur', run: () => o.moderate({ mic_off: mic, sound_off: !son }) })
  }
  return items
}
