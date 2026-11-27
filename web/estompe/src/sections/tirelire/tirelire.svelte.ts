import { lots, messageErreur, SANS_FILTRE, tirelireApi, type Categorie, type ClientTirelire, type Commercant, type Compteur, type FileAClasser, type Filtres, type Groupe, type Issue, type Operation, type Portee, type Regle, type SaisieCategorie, type SaisieRegle } from './tirelire'
import type { Releve } from './tirelire'

export interface Annulable { titre: string; detail: string; commercant: number | null; operation: number | null }

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
  categories = $state.raw<Categorie[] | null>(null)
  compteur = $state.raw<Compteur | null>(null)
  file = $state.raw<FileAClasser | null>(null)
  regles = $state.raw<Regle[] | null>(null)
  commercants = $state.raw<Commercant[] | null>(null)
  detail = $state.raw<Operation | null>(null)
  detailAbsent = $state(false)
  erreurClassement = $state('')
  occupe = $state(false)
  dernier = $state.raw<Annulable | null>(null)
  classeesIci = $state(0)
  passes = $state.raw<string[]>([])
  enTete = $state('')
  private recherche = ''
  private tourReference = 0
  private tourDetail = 0
  private sources = new Map<Issue, File>()
  private tour = 0
  private tourOperations = 0

  constructor(private api: ClientTirelire = tirelireApi) {}

  get filtre() { const f = this.filtres; return f.releve !== null || !!f.du || !!f.au || f.categorie !== null || f.commercant !== null || f.aClasser }
  get aClasser() { return this.compteur?.operations ?? 0 }
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
    this.tourReference++
    this.tourDetail++
    this.categories = null
    this.compteur = null
    this.file = null
    this.regles = null
    this.commercants = null
    this.detail = null
    this.detailAbsent = false
    this.erreurClassement = ''
    this.occupe = false
    this.dernier = null
    this.classeesIci = 0
    this.passes = []
    this.enTete = ''
    this.recherche = ''
  }

  async ouvrir() {
    if (this.releves) return
    await Promise.all([this.chargerReleves(), this.chargerReference()])
  }

  async chargerReference(avecCompteur = true) {
    const tour = ++this.tourReference
    try {
      const [categories, compteur] = await Promise.all([this.api.categories(), avecCompteur ? this.api.compteur() : null])
      if (tour !== this.tourReference) return
      this.categories = categories
      if (compteur) this.compteur = compteur
    } catch (e) {
      if (tour === this.tourReference && !this.categories) this.erreurClassement = messageErreur(e)
    }
  }

  lireCommercant(id: number) { return this.api.commercant(id) }

  categorie(id: number | null) { return id === null ? null : (this.categories?.find((c) => c.id === id) ?? null) }

  async chargerFile() {
    this.erreurClassement = ''
    try {
      const file = await this.api.aClasser()
      this.file = file
      this.compteur = { operations: file.operations, merchants: file.merchants, debits: file.debits, credits: file.credits }
    } catch (e) {
      this.erreurClassement = messageErreur(e)
    }
  }

  async chargerRegles() {
    try { this.regles = await this.api.regles() } catch (e) { this.erreurClassement = messageErreur(e) }
  }

  async chargerCommercants(q = this.recherche) {
    this.recherche = q
    try { this.commercants = await this.api.commercants(q) } catch (e) { this.erreurClassement = messageErreur(e) }
  }

  async chargerDetail(id: number) {
    const tour = ++this.tourDetail
    this.detailAbsent = false
    this.erreurOperations = ''
    this.detail = this.operations?.find((o) => o.id === id) ?? (this.detail?.id === id ? this.detail : null)
    if (this.detail) return
    try {
      const o = await this.api.operation(id)
      if (tour === this.tourDetail) this.detail = o
    } catch (e) {
      if (tour !== this.tourDetail) return
      if ((e as { status?: number }).status === 404) this.detailAbsent = true
      else this.erreurOperations = messageErreur(e)
    }
  }

  private async agir<T>(action: () => Promise<T>): Promise<T | null> {
    if (this.occupe) return null
    this.occupe = true
    this.erreurClassement = ''
    try {
      return await action()
    } catch (e) {
      this.erreurClassement = messageErreur(e)
      return null
    } finally {
      this.occupe = false
    }
  }

  private async apresClassement(file = false) {
    this.operations = null
    const avecFile = file || !!this.file
    await Promise.all([this.chargerReference(!avecFile), avecFile ? this.chargerFile() : null, this.regles ? this.chargerRegles() : null, this.commercants ? this.chargerCommercants() : null])
  }

  cleDe(g: Groupe) { return g.merchant_id !== null ? `m${g.merchant_id}` : `o${g.sample[0]?.id}` }

  get groupes(): Groupe[] {
    const rang = (g: Groupe) => (this.cleDe(g) === this.enTete ? 0 : this.passes.includes(this.cleDe(g)) ? 2 : 1)
    return [...(this.file?.groups ?? [])].sort((x, y) => rang(x) - rang(y))
  }

  passer(g: Groupe) {
    const cle = this.cleDe(g)
    this.passes = [...this.passes.filter((c) => c !== cle), cle]
    if (this.enTete === cle) this.enTete = ''
  }

  choisir(g: Groupe) { this.enTete = this.cleDe(g) }

  async classerGroupe(g: Groupe, categorie: Categorie, tout: boolean): Promise<boolean> {
    const op = g.sample[0]
    const res = await this.agir(async () => {
      if (tout && g.merchant_id !== null) {
        const r = await this.api.classerCommercant(g.merchant_id, categorie.id)
        return { titre: 'Règle créée pour tout le commerçant', detail: `« ${g.merchant_name ?? ''} » → ${categorie.name} · ${r.reclassified > 1 ? `${r.reclassified} opérations reclassées` : `${r.reclassified} opération reclassée`}`, commercant: g.merchant_id, operation: null, n: r.reclassified }
      }
      await this.api.classerOperation(op.id, categorie.id, 'operation')
      return { titre: 'Opération classée', detail: `« ${op.merchant_name ?? op.label} » → ${categorie.name} · cette opération seulement`, commercant: null, operation: op.id, n: 1 }
    })
    if (!res) return false
    this.dernier = { titre: res.titre, detail: res.detail, commercant: res.commercant, operation: res.operation }
    this.classeesIci += res.n
    await this.apresClassement(true)
    return true
  }

  async annuler() {
    const d = this.dernier
    if (!d) return
    const avant = this.aClasser
    const ok = await this.agir(async () => {
      if (d.commercant !== null) await this.api.classerCommercant(d.commercant, null)
      else if (d.operation !== null) await this.api.classerOperation(d.operation, null, 'operation')
      return true
    })
    if (!ok) return
    this.dernier = null
    await this.apresClassement(true)
    this.classeesIci = Math.max(0, this.classeesIci - Math.max(0, this.aClasser - avant))
  }

  async reclasser(o: Operation, categorie: number | null, portee: Portee): Promise<number | null> {
    const r = await this.agir(() => this.api.classerOperation(o.id, categorie, portee))
    if (!r) return null
    this.detail = r.operation
    await this.apresClassement()
    return r.reclassified
  }

  async enregistrerCategorie(id: number | null, c: Partial<SaisieCategorie & { archived: boolean }>): Promise<boolean> {
    const r = await this.agir(() => (id === null ? this.api.creerCategorie(c as SaisieCategorie) : this.api.modifierCategorie(id, c)))
    if (!r) return false
    await this.chargerReference()
    return true
  }

  async enregistrerRegle(id: number | null, r: SaisieRegle): Promise<number | null> {
    const res = await this.agir(() => (id === null ? this.api.creerRegle(r) : this.api.modifierRegle(id, r)))
    if (!res) return null
    if (!this.regles) await this.chargerRegles()
    await this.apresClassement()
    return res.reclassified
  }

  async supprimerRegle(id: number): Promise<number | null> {
    const res = await this.agir(() => this.api.supprimerRegle(id))
    if (!res) return null
    if (!this.regles) await this.chargerRegles()
    await this.apresClassement()
    return res.reclassified
  }

  async ordonnerRegles(ids: number[]): Promise<boolean> {
    const res = await this.agir(() => this.api.ordonnerRegles(ids))
    if (!res) return false
    this.regles = res.rules
    await this.apresClassement()
    return true
  }

  async classerCommercant(id: number, categorie: number | null): Promise<number | null> {
    const res = await this.agir(() => this.api.classerCommercant(id, categorie))
    if (!res) return null
    await this.apresClassement()
    return res.reclassified
  }

  async fusionner(id: number, autres: number[]): Promise<boolean> {
    const res = await this.agir(() => this.api.fusionner(id, autres))
    if (!res) return false
    await this.apresClassement()
    return true
  }

  async separer(id: number, cle: string): Promise<boolean> {
    const res = await this.agir(() => this.api.separer(id, [cle]))
    if (!res) return false
    await this.apresClassement()
    return true
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

  operation(id: number) { return this.operations?.find((o) => o.id === id) ?? (this.detail?.id === id ? this.detail : null) }
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
    this.file = null
    this.commercants = null
    await Promise.all([this.chargerReleves(), this.chargerReference()])
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
    this.file = null
    this.commercants = null
    this.detail = null
    await Promise.all([this.chargerReleves(), this.chargerReference()])
    return true
  }
}

export const tirelire = new EtatTirelire()
