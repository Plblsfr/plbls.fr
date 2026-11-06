import { cairnApi } from '../../lib/api'
import { cairnError, type CairnSpace, type CairnUser, type PageItem } from './cairn'

/** Droits et espaces Cairn de l'utilisateur ; l'entrée Cairn n'apparaît qu'avec `space.read`. */
class CairnState {
  actions = $state<string[]>([])
  spaces = $state<CairnSpace[]>([])
  space = $state('')
  /** Comptes vus dans les recherches : Cairn ne renvoie que des identifiants. */
  people = $state<Record<string, string>>({})

  /** Pages de l'espace courant : l'arbre de la barre latérale et la page affichée partagent cette liste. */
  pages = $state<PageItem[]>([])
  pagesError = $state('')
  /** Pages repliées dans l'arbre. */
  closed = $state<Record<string, boolean>>({})
  /** Création d'une page en cours (parent = null : à la racine). */
  creating = $state<{ parent: string | null } | null>(null)

  can = (action: string) => this.actions.includes(action)
  get visible() { return this.can('space.read') }

  async init() {
    try {
      this.actions = (await cairnApi.permissions()).actions
      if (this.visible) await this.loadSpaces()
    } catch {
      this.actions = [] // Cairn absent, injoignable ou session sans cookie : l'entrée reste masquée
    }
  }

  async loadSpaces() {
    this.spaces = await cairnApi.spaces()
    if (!this.spaces.some((s) => s.key === this.space)) this.space = (this.spaces.find((s) => !s.archived) ?? this.spaces[0])?.key ?? ''
  }

  async loadPages() {
    const space = this.space
    if (!space || !this.can('page.read')) return void (this.pages = [])
    try {
      const list = await cairnApi.pages(space)
      if (space === this.space) { this.pages = list; this.pagesError = '' }
    } catch (e) { this.pagesError = cairnError(e) }
  }

  remember(users: CairnUser[]) { for (const u of users) this.people[u.id] = u.email }
  who(id: string | null, me?: string) { return !id ? '' : id === me ? 'moi' : (this.people[id] ?? 'un autre compte') }
}

export const cairn = new CairnState()
