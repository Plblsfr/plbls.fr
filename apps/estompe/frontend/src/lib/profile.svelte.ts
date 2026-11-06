/** Profil ouvert (null = fermé) ; le Shell affiche la carte. `at` : point cliqué, où ancrer la carte (null : centrée). */
export const profileNav = $state<{ open: { userId: string; serverId: string; at: { x: number; y: number } | null } | null }>({ open: null })

// Dernier point de clic : la carte s'ouvre à côté de l'élément cliqué sans que chaque appelant ait à le transmettre.
let last: { x: number; y: number } | null = null
if (typeof window !== 'undefined') window.addEventListener('pointerdown', (e) => (last = { x: e.clientX, y: e.clientY }), true)

export const openProfile = (userId: string, serverId = '') => { profileNav.open = { userId, serverId, at: last } }

/** Position de la carte de largeur w et hauteur h près du point (x, y), gardée dans la fenêtre. */
export function placeCard(at: { x: number; y: number }, w: number, h: number, vw: number, vh: number, marge = 8) {
  const left = at.x + 12 + w + marge <= vw ? at.x + 12 : at.x - 12 - w
  return { left: Math.max(marge, Math.min(left, vw - w - marge)), top: Math.max(marge, Math.min(at.y - 24, vh - h - marge)) }
}
