import { tick } from 'svelte'

export type Point = [t: number, ms: number]
export interface DonneesSonde { images: Point[]; messages: Point[]; saisie: Point[] }

const vide = (): DonneesSonde => ({ images: [], messages: [], saisie: [] })
const apresAffichage = (f: () => void) => requestAnimationFrame(() => requestAnimationFrame(f))
const estSaisie = (cible: EventTarget | null) => cible instanceof HTMLElement && (cible.isContentEditable || cible.matches('input, textarea'))

class Sonde {
  actif = false
  premiereConversation: number | null = null
  private attendue = false
  private donnees = vide()
  private boucle = 0
  private precedente = 0
  private retirer: (() => void) | null = null

  conversationAffichee() {
    if (this.attendue) return
    this.attendue = true
    apresAffichage(() => { this.premiereConversation = Date.now() })
  }

  demarrer() {
    if (this.actif) return
    this.actif = true
    this.donnees = vide()
    const touche = (e: KeyboardEvent) => {
      if (!estSaisie(e.target)) return
      const t0 = e.timeStamp
      apresAffichage(() => { if (this.actif) this.donnees.saisie.push([Date.now(), performance.now() - t0]) })
    }
    document.addEventListener('keydown', touche, true)
    this.retirer = () => document.removeEventListener('keydown', touche, true)
  }

  suivreImages(oui: boolean) {
    cancelAnimationFrame(this.boucle)
    this.precedente = 0
    if (!oui || !this.actif) return
    const image = (t: number) => {
      if (!this.actif) return
      if (this.precedente) this.donnees.images.push([Date.now(), t - this.precedente])
      this.precedente = t
      this.boucle = requestAnimationFrame(image)
    }
    this.boucle = requestAnimationFrame(image)
  }

  reception() {
    if (!this.actif) return
    const t0 = performance.now()
    void tick().then(() => apresAffichage(() => { if (this.actif) this.donnees.messages.push([Date.now(), performance.now() - t0]) }))
  }

  releve(): DonneesSonde { return this.donnees }

  arreter(): DonneesSonde {
    this.suivreImages(false)
    this.actif = false
    this.retirer?.()
    this.retirer = null
    const d = this.donnees
    this.donnees = vide()
    return d
  }
}

export const sonde = new Sonde()
