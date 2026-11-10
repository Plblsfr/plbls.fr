import type { Capacites, Me } from './types'

export type Right = 'access' | 'admin' | 'movies' | 'acces' | 'users'

export const ADMIN_SECTIONS = [
  { id: 'users', label: 'utilisateurs', icon: 'users', right: 'admin', cap: 'estompe:admin.users' },
  { id: 'servers', label: 'serveurs', icon: 'monitor', right: 'admin', cap: 'estompe:admin.view' },
  { id: 'badges', label: 'badges', icon: 'pin', right: 'admin', cap: 'estompe:admin.badges' },
  { id: 'reports', label: 'signalements', icon: 'flag', right: 'admin', cap: 'estompe:admin.reports' },
  { id: 'bot', label: 'bot', icon: 'mail', right: 'admin', cap: 'estompe:admin.bot' },
  { id: 'stats', label: 'statistiques', icon: 'chart', right: 'admin', cap: 'estompe:admin.view' },
  { id: 'logs', label: 'logs', icon: 'file', right: 'admin', cap: 'estompe:admin.logs' },
  { id: 'films', label: 'fichiers écartés', icon: 'clap', right: 'admin', block: 'films', cap: 'estompe:admin.films' },
  { id: 'acces', label: 'accès', icon: 'cadenas', right: 'acces' },
] as const satisfies readonly { id: string; label: string; icon: string; right: Right; block?: string; cap?: string }[]
export type AdminSection = (typeof ADMIN_SECTIONS)[number]['id']
type Section = (typeof ADMIN_SECTIONS)[number]

export const rightsOf = (me: Me): Right[] =>
  me.rights ?? [...(me.allowed ? (['access'] as const) : []), ...(me.admin ? (['admin'] as const) : []), ...(me.movies ? (['movies'] as const) : [])]

export const autorise = (caps: Capacites | null | undefined, nom: string, ressource = '*'): boolean => {
  if (!caps) return true
  const c = caps[nom]
  if (!c || c.deny.includes('*') || c.deny.includes(ressource)) return false
  return c.allow.includes('*') || c.allow.includes(ressource)
}

export const adminSections = (rights: readonly Right[], on: (block: string) => boolean = () => true, caps: Capacites | null = null): Section[] =>
  ADMIN_SECTIONS.filter((s) => rights.includes(s.right) && (!('block' in s) || on(s.block)) && (!('cap' in s) || autorise(caps, s.cap)))

export const adminSection = (id: string | undefined, sections: readonly Section[]): AdminSection | undefined =>
  (sections.find((s) => s.id === id) ?? sections[0])?.id
