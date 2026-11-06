/** Navigateur et PWA : tout ce que le frontend utilise de la plateforme, sans code natif. */
import type { Platform } from '../../src/lib/platform'
import { isStandalone } from './browser'
import Downloads from './Downloads.svelte'
import { notifications } from './notifications'
import UpdateSettings from './UpdateSettings.svelte'
import { updates } from './updates'

/** Capture de l'onglet courant, après accord de l'utilisateur. */
async function captureTab(): Promise<Blob | null> {
  const stream = await navigator.mediaDevices.getDisplayMedia({ video: true, preferCurrentTab: true } as DisplayMediaStreamOptions)
  try {
    const video = document.createElement('video')
    video.srcObject = stream
    video.muted = true
    await video.play()
    await new Promise((r) => setTimeout(r, 300)) // la première image n'est pas toujours prête
    const canvas = document.createElement('canvas')
    canvas.width = video.videoWidth
    canvas.height = video.videoHeight
    canvas.getContext('2d')!.drawImage(video, 0, 0)
    return await new Promise<Blob | null>((r) => canvas.toBlob(r, 'image/jpeg', 0.85))
  } finally {
    stream.getTracks().forEach((t) => t.stop())
  }
}

export const platform: Platform = {
  id: () => (isStandalone() ? 'pwa' : 'web'),
  updates,
  notifications,
  screenshot: 'getDisplayMedia' in (navigator.mediaDevices ?? {}) ? { auto: false, capture: captureTab } : undefined,
  spaces: [],
  slots: { login: Downloads, settingsUpdate: UpdateSettings },
  start() { void import('virtual:pwa-register').then((m) => m.registerSW({ immediate: true })) },
  windows: {},
  restoresLastView: isStandalone,
  openFromNotification: () => false,
  cancelNotification() {},
  reportShortcut: true,
  micHelp: ' Autorisez-le dans les réglages du navigateur ou du système (sur Mac : Réglages Système › Confidentialité et sécurité › Microphone).',
  label: () => (isStandalone() ? 'PWA' : 'navigateur'),
  pttNote: "La touche ne fonctionne que lorsque l'onglet Estompe est actif et qu'aucun champ de saisie n'a le focus. Pour parler depuis un jeu ou une autre fenêtre, utilise l'application PC.",
}
