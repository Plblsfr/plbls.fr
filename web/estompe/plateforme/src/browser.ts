/** Navigateur Android (hors app) : on y propose l'APK plutôt que l'installeur Windows. */
export const isAndroidBrowser = () => /Android/.test(navigator.userAgent)

export const isStandalone = () => typeof matchMedia !== 'undefined' && matchMedia('(display-mode: standalone)').matches

export const installeur = (fichier: string) => `/updates/${fichier}?v=${encodeURIComponent(__APP_VERSION__)}`
