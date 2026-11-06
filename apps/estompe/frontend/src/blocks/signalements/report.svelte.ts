import { platform } from '../../lib/platform'

/** `shot` : capture de la fenêtre prise avant l'ouverture du formulaire (app Windows et Android), pour qu'il n'y figure pas. */
export const reportNav = $state<{ open: boolean; shot: Blob | null }>({ open: false, shot: null })

export async function openReport() {
  reportNav.shot = platform.screenshot?.auto ? await platform.screenshot.capture().catch(() => null) : null
  reportNav.open = true
}
