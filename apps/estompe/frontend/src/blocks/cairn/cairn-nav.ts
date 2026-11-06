/** Sections de l'espace Cairn, filtrées par les droits : une action non permise n'est pas proposée. */
export function cairnSections(can: (action: string) => boolean) {
  const all = [
    { id: 'mine', label: 'Mes tickets', icon: 'users', ok: can('ticket.read'), active: (s?: string) => s === 'mine' },
    { id: 'tickets', label: 'Tous les tickets', icon: 'hash', ok: can('ticket.read'), active: (s?: string) => s === 'tickets' || s === 'ticket' },
    { id: 'docs', label: 'Documentation', icon: 'file', ok: can('page.read'), active: (s?: string) => s === 'docs' },
    { id: 'search', label: 'Recherche', icon: 'search', ok: can('ticket.read') && can('page.read'), active: (s?: string) => s === 'search' },
    { id: 'trash', label: 'Corbeille', icon: 'trash', ok: can('page.delete'), active: (s?: string) => s === 'trash' },
  ] as const
  return all.filter((s) => s.ok)
}
