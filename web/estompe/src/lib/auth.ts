import { config } from './config'
import { clearLastView } from './lastview'
import { platform } from './platform'

const GATEWAY = config.gatewayUrl

/** Page de connexion Keycloak via auth-gateway, puis retour sur l'app. */
export const loginUrl = (back = location.href) => `${GATEWAY}/auth/login?redirect=${encodeURIComponent(back)}`
export const registerUrl = (back = location.href) => `${GATEWAY}/auth/register?redirect=${encodeURIComponent(back)}`

/** Déconnexion : POST vers le gateway (le cookie de session est sur .plbls.fr), qui redirige ensuite. */
export async function logout(back = location.origin) {
  clearLastView()
  if (platform.auth) {
    await platform.auth.logout()
    location.reload() // retour à l'écran de connexion
    return
  }
  const f = document.createElement('form')
  f.method = 'POST'
  f.action = `${GATEWAY}/auth/logout?redirect=${encodeURIComponent(back)}`
  document.body.append(f)
  f.submit()
}

