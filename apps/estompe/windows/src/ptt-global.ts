/** Raccourci global du push-to-talk (app Windows) : la touche reste active quand une autre fenêtre a le focus. */
import type { GlobalPttDriver } from '../../../../web/estompe/src/lib/platform'
const failure = (code: string) => `La touche ${code.replace(/^(Key|Digit)/, '')} ne peut pas servir de raccourci global : elle est déjà prise par une autre application, ou non prise en charge. Choisis-en une autre.`

/** Enregistre `code` (KeyboardEvent.code) ; `onChange(true)` à l'appui, `onChange(false)` au relâchement. Renvoie la fonction qui le libère. */
export async function registerGlobalPtt(code: string, onChange: (on: boolean) => void): Promise<() => Promise<void>> {
  const { register, unregister } = await import('@tauri-apps/plugin-global-shortcut')
  try {
    await register(code, (e) => onChange(e.state === 'Pressed'))
  } catch {
    throw new Error(failure(code))
  }
  return () => unregister(code).catch(() => {})
}

/** Vérifie qu'une touche peut devenir un raccourci global (message d'erreur, ou '' si elle convient). */
export async function checkGlobalPtt(code: string): Promise<string> {
  try {
    await (await registerGlobalPtt(code, () => {}))()
    return ''
  } catch (e) {
    return (e as Error).message
  }
}

export const globalPtt: GlobalPttDriver = { register: registerGlobalPtt, check: checkGlobalPtt }
