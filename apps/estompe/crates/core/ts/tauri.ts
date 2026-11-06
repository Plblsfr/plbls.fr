/** Appels aux commandes Rust (crates/core) communs aux apps Windows et Android. */
import type { InvokeArgs } from '@tauri-apps/api/core'
import { config } from '../../../../../web/estompe/src/lib/config'
import { authUrl, challengeS256, completeLogin, randomString, type PendingLogin, type TokenManager } from './desktop-auth'
import type { DebugStore } from '../../../../../web/estompe/src/lib/platform'

export async function invoke<T = void>(cmd: string, args?: InvokeArgs): Promise<T> {
  const { invoke: call } = await import('@tauri-apps/api/core')
  return call<T>(cmd, args)
}

/** fetch fait par Rust (commande native_fetch) : sans en-tête Origin, que Keycloak refuse depuis le webview. */
export async function nativeFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const headers = Object.entries((init?.headers ?? {}) as Record<string, string>)
  const r = await invoke<{ status: number; body: string }>('native_fetch', { method: init?.method ?? 'GET', url: String(input), headers, body: init?.body ? String(init.body) : null })
  return new Response([204, 205, 304].includes(r.status) ? null : r.body, { status: r.status })
}

/** Journal de debug dans un fichier géré par Rust (rotation). */
export const debugStore: DebugStore = {
  set: (enabled) => invoke('debug_set', { enabled }),
  read: () => invoke<string>('debug_read'),
  write: (line) => void invoke('debug_write', { line }).catch(() => {}),
  clear: () => invoke('debug_clear'),
}

/** Connexion Authorization Code + PKCE dans le navigateur système ; `wait` renvoie la query string de la redirection, `onStart` reçoit la connexion avant l'ouverture du navigateur. */
export async function browserLogin(
  manager: TokenManager,
  o: { issuer: string; clientId: string; redirectUri: string; register: boolean; wait: () => Promise<string>; onStart?: (p: PendingLogin) => void },
): Promise<void> {
  if (!o.issuer) throw new Error('Émetteur Keycloak non configuré (keycloakIssuer)')
  const pending: PendingLogin = { state: randomString(16), verifier: randomString(48), redirectUri: o.redirectUri }
  const url = authUrl({ issuer: o.issuer, clientId: o.clientId, redirectUri: o.redirectUri, state: pending.state, challenge: await challengeS256(pending.verifier), register: o.register })
  o.onStart?.(pending)
  const { openUrl } = await import('@tauri-apps/plugin-opener')
  await openUrl(url)
  await completeLogin(manager, await o.wait(), pending)
}

/** Après une connexion qui n'aboutit pas : demande au gateway pourquoi il refuse le jeton (message affiché à l'utilisateur). */
export async function diagnose(manager: TokenManager | null): Promise<string> {
  try {
    const token = await manager?.getAccessToken()
    if (!token) return 'aucun jeton obtenu de Keycloak'
    const res = await nativeFetch(`${config.gatewayUrl}/auth/me`, { headers: { Authorization: `Bearer ${token}` } })
    return `le gateway répond ${res.status} : ${(await res.text()).slice(0, 200)}`
  } catch (e) {
    return String(e)
  }
}
