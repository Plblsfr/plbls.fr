/** `sep` : trait de séparation au-dessus ; `checked` : entrée en cours (coche). */
export type MenuItem = { label: string; run: () => void; danger?: boolean; sep?: boolean; checked?: boolean }

/** `up` : le menu s'ouvre vers le haut, son bord bas à `y` (ancré à un élément en bas d'écran). */
export const ctxmenu = $state<{ at: { x: number; y: number; items: MenuItem[]; up?: boolean; haut?: number; opener?: Element } | null }>({ at: null })

/** Ouvre un menu ancré sous l'élément (au-dessus avec `up`). */
export function menuFrom(el: Element, items: MenuItem[], up = false) {
  const r = el.getBoundingClientRect()
  ctxmenu.at = { x: r.left, y: up ? r.top : r.bottom, items, up, haut: r.top, opener: el }
}

// Ordonnée du menu de hauteur h : sous l'ancre (y) ; au-dessus (haut = bord haut de l'ancre) s'il manque de place en bas et qu'il en reste en haut.
export function flipY(y: number, h: number, vh: number, haut: number | undefined, marge = 8) {
  return haut !== undefined && y + h > vh - marge && haut - h >= marge ? haut - h : y
}

// Garde le menu de taille w×h entièrement dans la fenêtre : il se replie vers le haut ou la gauche près des bords.
export function placeMenu(x: number, y: number, w: number, h: number, vw: number, vh: number, marge = 8) {
  return { left: Math.max(marge, Math.min(x, vw - w - marge)), top: Math.max(marge, Math.min(y, vh - h - marge)) }
}

// Texte sélectionné à l'intérieur de `node` (vide si la sélection est ailleurs).
export function selectionIn(node: Node): string {
  const s = getSelection()
  return s && !s.isCollapsed && node.contains(s.anchorNode) && node.contains(s.focusNode) ? s.toString() : ''
}

export function selectAll(node: Node) {
  const s = getSelection()
  const r = document.createRange()
  r.selectNodeContents(node)
  s?.removeAllRanges()
  s?.addRange(r)
}

// Le menu natif reste sur les champs de saisie et les liens situés dans l'élément (copier-coller, ouvrir le lien).
const NATIVE = 'input, textarea, select, a[href], [contenteditable="true"]'

// Svelte action : clic droit sur l'élément → menu personnalisé ; `items` renvoie les entrées (vide : menu natif).
export function ctx(node: HTMLElement, items: () => MenuItem[]) {
  const open = (e: MouseEvent) => {
    const t = (e.target as Element).closest(NATIVE)
    const list = items()
    if ((t && t !== node) || !list.length) return
    e.preventDefault()
    e.stopPropagation()
    ctxmenu.at = { x: e.clientX, y: e.clientY, items: list }
  }
  node.addEventListener('contextmenu', open)
  return { destroy: () => node.removeEventListener('contextmenu', open) }
}
