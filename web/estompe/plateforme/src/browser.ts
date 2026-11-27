/** Navigateur Android (hors app) : on y propose l'APK plutôt que l'installeur Windows. */
export const isAndroidBrowser = () => /Android/.test(navigator.userAgent)

export const isStandalone = () => typeof matchMedia !== 'undefined' && matchMedia('(display-mode: standalone)').matches

export const installeur = (fichier: string) => `/updates/${fichier}?v=${encodeURIComponent(__APP_VERSION__)}`

export async function partagerImage(image: Blob, nom: string, texte: string): Promise<boolean> {
  const fichier = new File([image], nom, { type: image.type || 'image/png' })
  if (typeof navigator.canShare !== 'function' || !navigator.canShare({ files: [fichier] })) return false
  try {
    await navigator.share({ files: [fichier], text: texte })
  } catch (e) {
    if (!(e instanceof DOMException && e.name === 'AbortError')) return false
  }
  return true
}
