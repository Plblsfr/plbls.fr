import { lots, messageErreur, tirelireApi, type ClientTirelire, type Filtres, type Issue, type Operation, type Releve } from './tirelire'

const SANS_FILTRE: Filtres = { releve: null, du: '', au: '' }

export class EtatTirelire {
  releves = $state.raw<Releve[] | null>(null)
  erreur = $state('')
  filtres = $state.raw<Filtres>(SANS_FILTRE)
  operations = $state.raw<Operation[] | null>(null)
  total = $state(0)
  suiteEnCours = $state(false)
  erreurOperations = $state('')
  envoi = $state(false)
  issues = $state.raw<Issue[]>([])
  erreurSuppression = $state('')
  private sources = new Map<Issue, File>()
  private tour = 0
  private tourOperations = 0

  constructor(private api: ClientTirelire = tirelireApi) {}

  get filtre() { return this.filtres.releve !== null || !!this.filtres.du || !!this.filtres.au }
  get reste() { return Math.max(0, this.total - (this.operations?.length ?? 0)) }

  reinitialiser() {
    this.tour++
    this.tourOperations++
    this.releves = null
    this.erreur = ''
    this.filtres = SANS_FILTRE
    this.operations = null
    this.total = 0
    this.suiteEnCours = false
    this.erreurOperations = ''
    this.envoi = false
    this.issues = []
    this.erreurSuppression = ''
    this.sources.clear()
  }

  async ouvrir() {
    if (this.releves) return
    await this.chargerReleves()
  }

  async chargerReleves() {
    const tour = ++this.tour
    this.erreur = ''
    try {
      const releves = await this.api.releves()
      if (tour === this.tour) this.releves = releves
    } catch (e) {
      if (tour === this.tour) this.erreur = messageErreur(e)
    }
  }

  async filtrer(f: Partial<Filtres>) {
    this.filtres = { ...this.filtres, ...f }
    await this.chargerOperations()
  }

  effacerFiltres() { return this.filtrer(SANS_FILTRE) }

  async chargerOperations() {
    const tour = ++this.tourOperations
    this.erreurOperations = ''
    this.suiteEnCours = false
    this.operations = null
    try {
      const page = await this.api.operations(this.filtres, 0)
      if (tour !== this.tourOperations) return
      this.operations = page.operations
      this.total = page.total
    } catch (e) {
      if (tour === this.tourOperations) this.erreurOperations = messageErreur(e)
    }
  }

  async suite() {
    if (!this.operations || this.suiteEnCours || !this.reste) return
    const tour = this.tourOperations
    this.suiteEnCours = true
    this.erreurOperations = ''
    try {
      const page = await this.api.operations(this.filtres, this.operations.length)
      if (tour !== this.tourOperations) return
      const connues = new Set(this.operations.map((o) => o.id))
      this.operations = [...this.operations, ...page.operations.filter((o) => !connues.has(o.id))]
      this.total = page.operations.length ? page.total : this.operations.length
    } catch (e) {
      if (tour === this.tourOperations) this.erreurOperations = messageErreur(e)
    } finally {
      if (tour === this.tourOperations) this.suiteEnCours = false
    }
  }

  operation(id: number) { return this.operations?.find((o) => o.id === id) ?? null }
  releve(id: number) { return this.releves?.find((r) => r.id === id) ?? null }

  private async envoyer(fichiers: File[]): Promise<Issue[]> {
    try {
      const resultats = await this.api.importer(fichiers)
      return fichiers.map((f, i) => {
        const r = resultats[i]
        return r ? { ...r, file: f.name } : { file: f.name, status: 'failed', message: 'Tirelire n’a rendu aucun résultat pour ce fichier.' }
      })
    } catch (e) {
      const message = messageErreur(e)
      return fichiers.map((f) => ({ file: f.name, status: 'failed', message }))
    }
  }

  private retenir(issues: Issue[], fichiers: File[]) {
    issues.forEach((issue, i) => { if (issue.status === 'failed' || issue.status === 'rejected') this.sources.set(issue, fichiers[i]) })
  }

  private async apresImport(issues: Issue[]) {
    if (!issues.some((i) => i.status === 'imported')) return
    this.operations = null
    await this.chargerReleves()
  }

  async importer(fichiers: File[]) {
    if (this.envoi || !fichiers.length) return
    this.envoi = true
    this.sources.clear()
    this.issues = []
    try {
      const plan = lots(fichiers)
      const parFichier = new Map<File, Issue>()
      for (const f of plan.tropGros) parFichier.set(f, { file: f.name, status: 'rejected', error: { code: 'too_large', message: 'fichier trop volumineux' } })
      for (const lot of plan.lots) {
        const issues = await this.envoyer(lot)
        this.retenir(issues, lot)
        lot.forEach((f, i) => parFichier.set(f, issues[i]))
        this.issues = fichiers.flatMap((f) => parFichier.get(f) ?? [])
      }
      this.issues = fichiers.flatMap((f) => parFichier.get(f) ?? [])
      await this.apresImport(this.issues)
    } finally {
      this.envoi = false
    }
  }

  peutReessayer(issue: Issue) { return this.sources.has(issue) }

  async reessayer(issue: Issue) {
    const fichier = this.sources.get(issue)
    if (this.envoi || !fichier) return
    this.envoi = true
    try {
      const [nouvelle] = await this.envoyer([fichier])
      this.sources.delete(issue)
      this.retenir([nouvelle], [fichier])
      this.issues = this.issues.map((i) => (i === issue ? nouvelle : i))
      await this.apresImport([nouvelle])
    } finally {
      this.envoi = false
    }
  }

  async supprimer(id: number): Promise<boolean> {
    this.erreurSuppression = ''
    try {
      await this.api.supprimer(id)
    } catch (e) {
      this.erreurSuppression = messageErreur(e)
      return false
    }
    this.issues = this.issues.filter((i) => !('statement' in i) || i.statement.id !== id)
    if (this.filtres.releve === id) this.filtres = { ...this.filtres, releve: null }
    this.operations = null
    await this.chargerReleves()
    return true
  }
}

export const tirelire = new EtatTirelire()
