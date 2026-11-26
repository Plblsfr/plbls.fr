import type { IconName } from '../../components/Icon.svelte'

export interface Onglet { id: string | undefined; label: string; long: string; icon: IconName }

export const ONGLETS: Onglet[] = [
  { id: undefined, label: 'Accueil', long: 'Accueil', icon: 'maison' },
  { id: 'mois', label: 'Mois', long: 'Vue du mois', icon: 'calendar' },
  { id: 'evolution', label: 'Évolution', long: 'Évolution', icon: 'tendance' },
  { id: 'habitudes', label: 'Habitudes', long: 'Habitudes', icon: 'repeat' },
  { id: 'operations', label: 'Opérations', long: 'Opérations', icon: 'list' },
]

export type Vue = 'accueil' | 'mois' | 'evolution' | 'habitudes' | 'operations' | 'operation' | 'import' | 'classer' | 'categories'
export type Volet = 'categories' | 'regles' | 'commercants'

export const voletDe = (id?: string): Volet => (id === 'regles' || id === 'commercants' ? id : 'categories')

export function vueDe(section?: string, id?: string): Vue {
  if (section === 'import') return 'import'
  if (section === 'a-classer') return 'classer'
  if (section === 'categories') return 'categories'
  if (section === 'operations') return id && /^\d+$/.test(id) ? 'operation' : 'operations'
  if (section === 'mois' || section === 'evolution' || section === 'habitudes') return section
  return 'accueil'
}

export const estOutil = (vue: Vue) => vue === 'import' || vue === 'classer' || vue === 'categories'

export const ongletActif = (vue: Vue): string | undefined => (vue === 'operation' || vue === 'operations' ? 'operations' : vue === 'accueil' || vue === 'import' || vue === 'classer' || vue === 'categories' ? undefined : vue)
