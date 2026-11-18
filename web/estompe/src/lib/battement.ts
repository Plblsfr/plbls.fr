import { PLATEFORMES, sablierApi, sablierOn, type Plateforme, type Section } from '../sections/sablier/sablier'
import { platform } from './platform'
import type { Phase } from './store.svelte'

export const PERIODE_DEFAUT = 30
const PERIODE_MIN = 5
const PERIODE_MAX = 3600

export const periodeValide = (secondes: unknown): number =>
  typeof secondes === 'number' && Number.isFinite(secondes) && secondes >= PERIODE_MIN && secondes <= PERIODE_MAX ? secondes : PERIODE_DEFAUT

export const plateformeDe = (id: string): Plateforme => PLATEFORMES.find((p) => p === id) ?? 'web'

const SECTION_DE_ROUTE = new Map<string, Section>([
  ['home', 'messagerie'],
  ['channel', 'messagerie'],
  ['dm', 'messagerie'],
  ['films', 'entracte'],
  ['musique', 'sillon'],
  ['regie', 'regie'],
  ['admin', 'admin'],
  ['stats', 'sablier'],
])

export const sectionDe = (route: string): Section => SECTION_DE_ROUTE.get(route) ?? 'autre'

export const sessionOuverte = (phase: Phase): boolean => phase === 'ready' || phase === 'sections' || phase === 'admin' || phase === 'activate' || phase === 'denied'

export const sectionOuverte = (phase: Phase, route: string): Section => {
  if (phase === 'admin') return 'admin'
  if (phase === 'ready' || phase === 'sections') return sectionDe(route)
  return 'autre'
}

interface Cible {
  addEventListener(type: string, ecouteur: () => void): void
  removeEventListener(type: string, ecouteur: () => void): void
}

export interface Environnement {
  doc: Cible & { readonly visibilityState: string; hasFocus(): boolean }
  fenetre: Cible
  minuteur: { poser(f: () => void, ms: number): unknown; retirer(id: unknown): void }
  envoyer(): Promise<unknown>
  periode?(): Promise<unknown>
}

export class Battement {
  private suivi = false
  private actif = false
  private cycle: unknown = null
  private secondes = PERIODE_DEFAUT

  constructor(private env: Environnement) {}

  private auPremierPlan() { return this.env.doc.visibilityState === 'visible' && this.env.doc.hasFocus() }

  private suspendre = () => {
    if (!this.actif) return
    this.actif = false
    this.env.minuteur.retirer(this.cycle)
    this.cycle = null
  }

  private battre = () => {
    if (!this.auPremierPlan()) { this.suspendre(); return }
    try { this.env.envoyer().catch(() => {}) } catch { return }
  }

  private armer() { this.cycle = this.env.minuteur.poser(this.battre, this.secondes * 1000) }

  private evaluer = () => {
    if (!this.suivi || !this.auPremierPlan()) { this.suspendre(); return }
    if (this.actif) return
    this.actif = true
    this.battre()
    this.armer()
  }

  private regler(secondes: unknown) {
    const valeur = periodeValide(secondes)
    if (valeur === this.secondes) return
    this.secondes = valeur
    if (!this.actif) return
    this.env.minuteur.retirer(this.cycle)
    this.armer()
  }

  private ecoutes(): [Cible, string, () => void][] {
    const { doc, fenetre } = this.env
    return [
      [doc, 'visibilitychange', this.evaluer],
      [doc, 'freeze', this.suspendre],
      [doc, 'resume', this.evaluer],
      [fenetre, 'focus', this.evaluer],
      [fenetre, 'blur', this.evaluer],
      [fenetre, 'pagehide', this.suspendre],
      [fenetre, 'pageshow', this.evaluer],
    ]
  }

  demarrer() {
    if (this.suivi) return
    this.suivi = true
    for (const [cible, type, f] of this.ecoutes()) cible.addEventListener(type, f)
    this.evaluer()
    try { this.env.periode?.().then((s) => this.regler(s), () => {}) } catch { return }
  }

  arreter() {
    if (!this.suivi) return
    this.suivi = false
    for (const [cible, type, f] of this.ecoutes()) cible.removeEventListener(type, f)
    this.suspendre()
  }
}

export function suivreUtilisation(section: () => Section): () => void {
  if (!sablierOn()) return () => {}
  const battement = new Battement({
    doc: document,
    fenetre: window,
    minuteur: { poser: (f, ms) => setInterval(f, ms), retirer: (id) => clearInterval(id as ReturnType<typeof setInterval>) },
    envoyer: () => sablierApi.battre(plateformeDe(platform.id()), section()),
    periode: async () => (await sablierApi.me()).heartbeat_seconds,
  })
  battement.demarrer()
  return () => battement.arreter()
}
