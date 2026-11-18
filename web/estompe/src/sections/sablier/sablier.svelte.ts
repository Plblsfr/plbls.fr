import { messageOf } from '../../lib/ui'
import { jourLocal } from './formats'
import { borner, precedente, suivante } from './periodes'
import { sablierApi, type ChoixPartage, type ClientSablier, type Domaine, type NonVus, type Partage, type Periode, type Recap, type RecapResume, type Series, type Stats, type TropheeDebloque, type Trophees } from './sablier'
import { estVide, PLUS_LARGE } from './stats'

export interface Ailleurs { periode: Periode; date: string }

const DOMAINES: Domaine[] = ['usage', 'entracte', 'sillon', 'messaging']
const INJOIGNABLE = 'Les stats sont injoignables pour le moment.'

export class EtatSablier {
  nonVus = $state.raw<NonVus>({ trophies: 0, recaps: 0 })
  periode = $state<Periode>('month')
  date = $state('')
  stats = $state.raw<Stats | null>(null)
  ailleurs = $state.raw<Partial<Record<Domaine, Ailleurs | null>>>({})
  trophees = $state.raw<Trophees | null>(null)
  series = $state.raw<Series | null>(null)
  premierJour = $state<string | null>(null)
  charge = $state(false)
  erreur = $state('')
  annonces = $state.raw<TropheeDebloque[]>([])
  recapAnnonce = $state.raw<RecapResume | null>(null)
  recaps = $state.raw<RecapResume[] | null>(null)
  erreurRecaps = $state('')
  partage = $state.raw<Partage | null>(null)
  erreurPartage = $state('')
  private tour = 0
  private recapPropose = false

  constructor(private api: ClientSablier = sablierApi, private jour: () => string = jourLocal) {
    this.date = this.jour()
  }

  get aujourdhui() { return this.jour() }
  get pastille() { return this.nonVus.trophies + this.nonVus.recaps > 0 }

  reinitialiser() {
    this.tour++
    this.nonVus = { trophies: 0, recaps: 0 }
    this.periode = 'month'
    this.date = this.jour()
    this.stats = null
    this.ailleurs = {}
    this.trophees = null
    this.series = null
    this.premierJour = null
    this.charge = false
    this.erreur = ''
    this.annonces = []
    this.recapAnnonce = null
    this.recaps = null
    this.erreurRecaps = ''
    this.partage = null
    this.erreurPartage = ''
    this.recapPropose = false
  }

  async chargerNonVus() {
    try { this.nonVus = await this.api.nonVus() } catch { return }
  }

  private lireStats = async (periode: Periode, date: string): Promise<Stats> => {
    const [usage, entracte, sillon, messaging] = await Promise.all([this.api.usage(periode, date), this.api.entracte(periode, date), this.api.sillon(periode, date), this.api.messagerie(periode, date)])
    return { usage, entracte, sillon, messaging }
  }

  private async chercherAilleurs(tour: number, stats: Stats, periode: Periode, date: string) {
    let vides = DOMAINES.filter((d) => estVide(stats, d))
    const trouve: Partial<Record<Domaine, Ailleurs | null>> = Object.fromEntries(vides.map((d) => [d, null]))
    let large = PLUS_LARGE[periode]
    while (vides.length && large) {
      let autres: Stats
      try { autres = await this.lireStats(large, date) } catch { break }
      if (tour !== this.tour) return
      for (const d of vides) if (!estVide(autres, d)) trouve[d] = { periode: large, date }
      vides = vides.filter((d) => !trouve[d])
      large = PLUS_LARGE[large]
    }
    if (tour === this.tour) this.ailleurs = trouve
  }

  async chargerStats() {
    const tour = ++this.tour
    const { periode, date } = this
    this.erreur = ''
    try {
      const stats = await this.lireStats(periode, date)
      if (tour !== this.tour) return
      this.stats = stats
      this.premierJour = stats.usage.first_day
      this.ailleurs = {}
      this.charge = true
      void this.chercherAilleurs(tour, stats, periode, date)
    } catch (e) {
      if (tour !== this.tour) return
      this.erreur = messageOf(e, INJOIGNABLE)
    }
  }

  private async chargerJeu() {
    const [trophees, series] = await Promise.all([this.api.trophees(), this.api.series()])
    this.trophees = trophees
    this.series = series
    this.annonces = trophees.unlocked.filter((t) => !t.seen)
  }

  async ouvrir() {
    this.erreur = ''
    try {
      await this.chargerJeu()
    } catch (e) {
      this.erreur = messageOf(e, INJOIGNABLE)
      return
    }
    await this.chargerStats()
    if (this.erreur) return
    await this.chargerNonVus()
    if (!this.recapPropose && this.nonVus.recaps > 0) {
      this.recapPropose = true
      await this.chargerRecaps()
      this.recapAnnonce = this.recaps?.find((r) => !r.seen) ?? null
    }
  }

  choisir(periode: Periode) {
    if (periode === this.periode) return
    this.periode = periode
    this.date = borner(this.date, this.premierJour, this.jour())
    void this.chargerStats()
  }

  aller(periode: Periode, date: string) {
    this.periode = periode
    this.date = borner(date, this.premierJour, this.jour())
    void this.chargerStats()
  }

  reculer() {
    const date = precedente(this.periode, this.date, this.premierJour)
    if (date === this.date) return
    this.date = date
    void this.chargerStats()
  }

  avancer() {
    const date = suivante(this.periode, this.date, this.jour())
    if (date === this.date) return
    this.date = date
    void this.chargerStats()
  }

  async tropheeVu(id: number) {
    this.annonces = this.annonces.filter((t) => t.id !== id)
    if (this.trophees) this.trophees = { ...this.trophees, unlocked: this.trophees.unlocked.map((t) => (t.id === id ? { ...t, seen: true } : t)) }
    try { await this.api.marquerVus({ trophies: [id] }) } catch { return }
    await this.chargerNonVus()
  }

  remettreRecap() { this.recapAnnonce = null }

  async chargerRecaps() {
    this.erreurRecaps = ''
    try { this.recaps = await this.api.recaps() } catch (e) { this.erreurRecaps = messageOf(e, INJOIGNABLE) }
  }

  async lireRecap(id: number): Promise<Recap> {
    const recap = await this.api.recap(id)
    this.recapAnnonce = null
    if (this.recaps) this.recaps = this.recaps.map((r) => (r.id === id ? { ...r, seen: true } : r))
    void this.api.marquerVus({ recaps: [id] }).then(() => this.chargerNonVus(), () => {})
    return recap
  }

  async chargerPartage() {
    this.erreurPartage = ''
    try { this.partage = await this.api.partage() } catch (e) { this.erreurPartage = messageOf(e, INJOIGNABLE) }
  }

  async partager(choix: ChoixPartage) {
    const reglages = await this.api.partager(choix)
    this.partage = reglages
    if (this.trophees && choix.trophies) {
      const partages = new Map(reglages.trophies.map((t) => [t.id, t.shared]))
      this.trophees = { ...this.trophees, unlocked: this.trophees.unlocked.map((t) => ({ ...t, shared: partages.get(t.id) ?? t.shared })) }
    }
  }
}

export const sablier = new EtatSablier()
