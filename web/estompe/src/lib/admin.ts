/** Sections de l'espace « Estompe » (administration), présentées comme des salons. */
export const ADMIN_SECTIONS = [
  { id: 'users', label: 'utilisateurs', icon: 'users' },
  { id: 'servers', label: 'serveurs', icon: 'monitor' },
  { id: 'badges', label: 'badges', icon: 'pin' },
  { id: 'reports', label: 'signalements', icon: 'flag' },
  { id: 'bot', label: 'bot', icon: 'mail' },
  { id: 'stats', label: 'statistiques', icon: 'chart' },
  { id: 'logs', label: 'logs', icon: 'file' },
  { id: 'films', label: 'fichiers écartés', icon: 'clap' },
] as const
export type AdminSection = (typeof ADMIN_SECTIONS)[number]['id']
export const adminSection = (id?: string): AdminSection => ADMIN_SECTIONS.find((s) => s.id === id)?.id ?? 'users'
