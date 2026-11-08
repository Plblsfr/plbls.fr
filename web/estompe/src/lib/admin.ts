import type { Me } from './types'

export type Right = 'access' | 'admin' | 'movies' | 'permissions' | 'users'

export const ADMIN_SECTIONS = [
  { id: 'users', label: 'utilisateurs', icon: 'users', right: 'admin' },
  { id: 'servers', label: 'serveurs', icon: 'monitor', right: 'admin' },
  { id: 'badges', label: 'badges', icon: 'pin', right: 'admin' },
  { id: 'reports', label: 'signalements', icon: 'flag', right: 'admin' },
  { id: 'bot', label: 'bot', icon: 'mail', right: 'admin' },
  { id: 'stats', label: 'statistiques', icon: 'chart', right: 'admin' },
  { id: 'logs', label: 'logs', icon: 'file', right: 'admin' },
  { id: 'films', label: 'fichiers écartés', icon: 'clap', right: 'admin', block: 'films' },
  { id: 'permissions', label: 'permissions', icon: 'cadenas', right: 'permissions' },
] as const satisfies readonly { id: string; label: string; icon: string; right: Right; block?: string }[]
export type AdminSection = (typeof ADMIN_SECTIONS)[number]['id']
type Section = (typeof ADMIN_SECTIONS)[number]

export const rightsOf = (me: Me): Right[] =>
  me.rights ?? [...(me.allowed ? (['access'] as const) : []), ...(me.admin ? (['admin'] as const) : []), ...(me.movies ? (['movies'] as const) : [])]

export const adminSections = (rights: readonly Right[], on: (block: string) => boolean = () => true): Section[] =>
  ADMIN_SECTIONS.filter((s) => rights.includes(s.right) && (!('block' in s) || on(s.block)))

export const adminSection = (id: string | undefined, sections: readonly Section[]): AdminSection | undefined =>
  (sections.find((s) => s.id === id) ?? sections[0])?.id
