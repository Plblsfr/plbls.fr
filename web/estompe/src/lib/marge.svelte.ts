export type OngletMarge = 'membres' | 'epingles'

export const MARGE_MIN = 260
export const MARGE_MAX = 480
const MARGE_DEFAUT = 320
const CLE = 'estompe.marge'
const LARGE = '(min-width: 1200px)'

export const borneLargeur = (l: number) => Math.round(Math.min(MARGE_MAX, Math.max(MARGE_MIN, l)))

export function lireMarge(brut: string | null): { ouverte: boolean; largeur: number } {
  try {
    const v = JSON.parse(brut ?? '') as { ouverte?: unknown; largeur?: unknown }
    return { ouverte: v.ouverte !== false, largeur: borneLargeur(Number(v.largeur) || MARGE_DEFAUT) }
  } catch {
    return { ouverte: true, largeur: MARGE_DEFAUT }
  }
}

export function ongletEffectif(voulu: OngletMarge, membres: boolean, epingles: boolean): OngletMarge | null {
  if (voulu === 'epingles' && epingles) return 'epingles'
  return membres ? 'membres' : null
}

class Marge {
  ouverte = $state(true)
  superposee = $state(false)
  onglet = $state<OngletMarge>('membres')
  largeur = $state(MARGE_DEFAUT)
  large = $state(true)
  private avant: { visible: boolean; onglet: OngletMarge } | null = null

  get visible(): boolean {
    return this.large ? this.ouverte : this.superposee
  }

  demarrer(): () => void {
    let stocke: string | null = null
    try { stocke = localStorage.getItem(CLE) } catch { stocke = null }
    const lu = lireMarge(stocke)
    this.ouverte = lu.ouverte
    this.largeur = lu.largeur
    if (typeof matchMedia !== 'function') return () => {}
    const mq = matchMedia(LARGE)
    const suivre = () => { this.large = mq.matches; this.superposee = false }
    suivre()
    mq.addEventListener('change', suivre)
    return () => mq.removeEventListener('change', suivre)
  }

  private enregistrer() {
    try { localStorage.setItem(CLE, JSON.stringify({ ouverte: this.ouverte, largeur: this.largeur })) } catch { void 0 }
  }

  private regler(visible: boolean) {
    if (this.large) { this.ouverte = visible; this.enregistrer() } else this.superposee = visible
  }

  basculer() { this.avant = null; this.regler(!this.visible) }
  fermer() { this.avant = null; this.regler(false) }
  montrer(onglet: OngletMarge) { this.onglet = onglet; this.regler(true) }
  fermerSuperposee() { this.superposee = false }

  basculerEpingles() {
    if (this.visible && this.onglet === 'epingles') {
      const a = this.avant
      this.avant = null
      this.onglet = a?.onglet ?? 'membres'
      if (!a?.visible) this.regler(false)
      return
    }
    this.avant = { visible: this.visible, onglet: this.onglet }
    this.montrer('epingles')
  }

  redimensionner(l: number) { this.largeur = borneLargeur(l); this.enregistrer() }
}

export const marge = new Marge()
