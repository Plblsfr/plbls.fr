/** Recherche de messages : fenêtre ouverte ou non, et conversation affichée (portée proposée par défaut). Renseigné par le Shell. */
export const searchNav = $state({ open: false, convKey: '', serverId: '' })

export const openSearch = () => { searchNav.open = true }
