/** Fenêtre visible d'une longue liste de lignes de hauteur variable (hauteurs mesurées, sinon estimées). */
export interface Window {
  start: number
  end: number // exclu
  top: number // hauteur des lignes avant `start`
  bottom: number // hauteur des lignes à partir de `end`
}

/**
 * @param keys identifiants des lignes, dans l'ordre
 * @param height hauteur connue d'une ligne, ou l'estimation
 * @param scrollTop position de défilement relative au début de la liste
 * @param viewport hauteur visible
 * @param overscan marge dessinée au-dessus et au-dessous de la zone visible
 */
export function windowOf(keys: string[], height: (key: string) => number, scrollTop: number, viewport: number, overscan = 1000): Window {
  const lo = scrollTop - overscan
  const hi = scrollTop + viewport + overscan
  let y = 0
  let start = -1
  let end = keys.length
  let top = 0
  let endY = 0
  for (let i = 0; i < keys.length; i++) {
    const h = height(keys[i])
    if (start < 0 && y + h > lo) {
      start = i
      top = y
    }
    if (end === keys.length && y > hi) {
      end = i
      endY = y
    }
    y += h
  }
  if (start < 0) {
    // défilement au-delà de la fin : on garde les dernières lignes
    start = Math.max(0, keys.length - 1)
    top = y - (keys.length ? height(keys[keys.length - 1]) : 0)
  }
  if (end === keys.length) endY = y
  return { start, end, top, bottom: y - endY }
}
