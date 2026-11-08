import { untrack } from 'svelte'
import { api } from './api'
import { messageOf, notify } from './ui'

export interface AppInfo { key: string; name: string; description: string }
export interface Permission { key: string; description: string }
export interface PermRole { key: string; name: string; description: string; permissions: string[] }
export interface Assignment { user_id: string; app: string; role: string; granted_by: string; created_at: string }
export interface PermAudit { at: string; actor: string; action: string; target: string; detail: unknown }
export interface FoundUsers { users: { id: string; email: string }[]; first: number; max: number; next: number | null }

export const RESERVEE = 'permission'
export const enc = encodeURIComponent
const ECHEC = 'Le service de permission a refusé la demande'

export const lire = <T>(chemin: string) => api.permissions<T>('GET', chemin)

export function executer(p: Promise<unknown>, fait?: string): Promise<boolean> {
  return p.then(() => { if (fait) notify.ok(fait); return true }, (e) => { notify.error(e, ECHEC); return false })
}

export interface Charge<T> { readonly data: T | null; readonly error: string | null; reload: () => void }

export function charger<T>(chemin: () => string | null): Charge<T> {
  let data = $state.raw<T | null>(null)
  let error = $state<string | null>(null)
  let courant: string | null = null
  const reload = () => {
    const c = courant
    if (!c) { data = null; return }
    error = null
    lire<T>(c).then(
      (d) => { if (courant === c) data = d },
      (e) => { if (courant === c) error = messageOf(e, ECHEC) },
    )
  }
  $effect(() => {
    const c = chemin()
    if (c === courant) return
    courant = c
    data = null
    untrack(reload)
  })
  return { get data() { return data }, get error() { return error }, reload }
}

export const TONS: Record<string, string> = { create: 'ok', grant: 'ok', update: 'info', delete: 'dgr', revoke: 'dgr' }
export const ton = (action: string) => TONS[action.split('.')[1]] ?? ''
