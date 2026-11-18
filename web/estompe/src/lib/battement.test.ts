import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Battement, PERIODE_DEFAUT, periodeValide, plateformeDe, sectionDe, sectionOuverte, sessionOuverte, type Environnement } from './battement'

class FauxDocument extends EventTarget {
  visibilityState = 'visible'
  focus = true
  hasFocus() { return this.focus }
}

function monter(options: { envoyer?: () => Promise<unknown>; periode?: () => Promise<unknown> } = {}) {
  const doc = new FauxDocument()
  const fenetre = new EventTarget()
  const envoyer = vi.fn(options.envoyer ?? (() => Promise.resolve()))
  const env: Environnement = {
    doc,
    fenetre,
    minuteur: { poser: (f, ms) => setInterval(f, ms), retirer: (id) => clearInterval(id as ReturnType<typeof setInterval>) },
    envoyer,
    periode: options.periode,
  }
  const battement = new Battement(env)
  const masquer = () => { doc.visibilityState = 'hidden'; doc.dispatchEvent(new Event('visibilitychange')) }
  const afficher = () => { doc.visibilityState = 'visible'; doc.dispatchEvent(new Event('visibilitychange')) }
  const perdreFocus = () => { doc.focus = false; fenetre.dispatchEvent(new Event('blur')) }
  const prendreFocus = () => { doc.focus = true; fenetre.dispatchEvent(new Event('focus')) }
  return { doc, fenetre, envoyer, battement, masquer, afficher, perdreFocus, prendreFocus }
}

const MS = PERIODE_DEFAUT * 1000

beforeEach(() => { vi.useFakeTimers() })
afterEach(() => { vi.useRealTimers() })

describe('battement de temps d\'utilisation', () => {
  it('bat aussitôt puis à chaque période tant que la page est visible et a le focus', () => {
    const m = monter()
    m.battement.demarrer()
    expect(m.envoyer).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(MS - 1)
    expect(m.envoyer).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(1)
    expect(m.envoyer).toHaveBeenCalledTimes(2)
    vi.advanceTimersByTime(MS * 3)
    expect(m.envoyer).toHaveBeenCalledTimes(5)
  })

  it('n\'envoie rien quand la page est masquée', () => {
    const m = monter()
    m.doc.visibilityState = 'hidden'
    m.battement.demarrer()
    vi.advanceTimersByTime(MS * 10)
    expect(m.envoyer).not.toHaveBeenCalled()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('n\'envoie rien quand la page n\'a pas le focus', () => {
    const m = monter()
    m.doc.focus = false
    m.battement.demarrer()
    vi.advanceTimersByTime(MS * 10)
    expect(m.envoyer).not.toHaveBeenCalled()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('s\'arrête aussitôt quand la page est masquée', () => {
    const m = monter()
    m.battement.demarrer()
    vi.advanceTimersByTime(MS)
    expect(m.envoyer).toHaveBeenCalledTimes(2)
    m.masquer()
    expect(vi.getTimerCount()).toBe(0)
    vi.advanceTimersByTime(MS * 10)
    expect(m.envoyer).toHaveBeenCalledTimes(2)
  })

  it('s\'arrête aussitôt à la perte du focus', () => {
    const m = monter()
    m.battement.demarrer()
    m.perdreFocus()
    expect(vi.getTimerCount()).toBe(0)
    vi.advanceTimersByTime(MS * 10)
    expect(m.envoyer).toHaveBeenCalledTimes(1)
  })

  it('s\'arrête quand la page est quittée ou gelée', () => {
    for (const [cible, type] of [['fenetre', 'pagehide'], ['doc', 'freeze']] as const) {
      const m = monter()
      m.battement.demarrer()
      m[cible].dispatchEvent(new Event(type))
      expect(vi.getTimerCount()).toBe(0)
      vi.advanceTimersByTime(MS * 10)
      expect(m.envoyer).toHaveBeenCalledTimes(1)
    }
  })

  it('reprend par un battement immédiat au retour de la visibilité et du focus', () => {
    const m = monter()
    m.battement.demarrer()
    m.masquer()
    m.perdreFocus()
    m.afficher()
    vi.advanceTimersByTime(MS * 2)
    expect(m.envoyer).toHaveBeenCalledTimes(1)
    m.prendreFocus()
    expect(m.envoyer).toHaveBeenCalledTimes(2)
    vi.advanceTimersByTime(MS - 1)
    expect(m.envoyer).toHaveBeenCalledTimes(2)
    vi.advanceTimersByTime(1)
    expect(m.envoyer).toHaveBeenCalledTimes(3)
  })

  it('ne double pas le cycle quand un signal confirme l\'état en cours', () => {
    const m = monter()
    m.battement.demarrer()
    m.prendreFocus()
    m.afficher()
    m.battement.demarrer()
    expect(m.envoyer).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(1)
  })

  it('n\'envoie pas si le focus est perdu sans signal', () => {
    const m = monter()
    m.battement.demarrer()
    m.doc.focus = false
    vi.advanceTimersByTime(MS * 3)
    expect(m.envoyer).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(0)
    m.prendreFocus()
    expect(m.envoyer).toHaveBeenCalledTimes(2)
  })

  it('avale un échec de l\'envoi et continue le cycle', async () => {
    let appels = 0
    const m = monter({ envoyer: () => { appels += 1; if (appels === 2) throw new Error('synchrone'); return Promise.reject(new Error('réseau')) } })
    m.battement.demarrer()
    await vi.advanceTimersByTimeAsync(MS * 3)
    expect(m.envoyer).toHaveBeenCalledTimes(4)
    expect(vi.getTimerCount()).toBe(1)
  })

  it('suit la période annoncée par le serveur', async () => {
    const m = monter({ periode: () => Promise.resolve(10) })
    m.battement.demarrer()
    await vi.advanceTimersByTimeAsync(0)
    expect(m.envoyer).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(10_000)
    expect(m.envoyer).toHaveBeenCalledTimes(2)
    vi.advanceTimersByTime(20_000)
    expect(m.envoyer).toHaveBeenCalledTimes(4)
  })

  it('garde 30 s quand la période est absente, invalide ou que sa lecture échoue', async () => {
    for (const periode of [() => Promise.resolve(undefined), () => Promise.resolve(0), () => Promise.reject(new Error('401'))]) {
      const m = monter({ periode })
      m.battement.demarrer()
      await vi.advanceTimersByTimeAsync(MS)
      expect(m.envoyer).toHaveBeenCalledTimes(2)
      m.battement.arreter()
    }
  })

  it('ne relance pas le cycle si la période arrive page masquée', async () => {
    const m = monter({ periode: () => Promise.resolve(10) })
    m.doc.visibilityState = 'hidden'
    m.battement.demarrer()
    await vi.advanceTimersByTimeAsync(MS)
    expect(m.envoyer).not.toHaveBeenCalled()
    m.afficher()
    vi.advanceTimersByTime(10_000)
    expect(m.envoyer).toHaveBeenCalledTimes(2)
  })

  it('n\'écoute plus rien après l\'arrêt', () => {
    const m = monter()
    m.battement.demarrer()
    m.battement.arreter()
    expect(vi.getTimerCount()).toBe(0)
    m.prendreFocus()
    m.afficher()
    m.fenetre.dispatchEvent(new Event('pageshow'))
    vi.advanceTimersByTime(MS * 3)
    expect(m.envoyer).toHaveBeenCalledTimes(1)
    m.battement.demarrer()
    expect(m.envoyer).toHaveBeenCalledTimes(2)
  })
})

describe('valeurs du battement', () => {
  it('borne la période', () => {
    expect(periodeValide(30)).toBe(30)
    expect(periodeValide(45)).toBe(45)
    for (const v of [undefined, null, '30', Number.NaN, Number.POSITIVE_INFINITY, 0, -5, 4, 3601]) expect(periodeValide(v)).toBe(PERIODE_DEFAUT)
  })

  it('ramène la plateforme à la liste fermée', () => {
    for (const p of ['web', 'pwa', 'windows', 'android']) expect(plateformeDe(p)).toBe(p)
    expect(plateformeDe('ios')).toBe('web')
    expect(plateformeDe('')).toBe('web')
  })

  it('déduit la section de la route', () => {
    expect(sectionDe('home')).toBe('messagerie')
    expect(sectionDe('channel')).toBe('messagerie')
    expect(sectionDe('dm')).toBe('messagerie')
    expect(sectionDe('films')).toBe('entracte')
    expect(sectionDe('musique')).toBe('sillon')
    expect(sectionDe('regie')).toBe('regie')
    expect(sectionDe('admin')).toBe('admin')
    expect(sectionDe('stats')).toBe('sablier')
    expect(sectionDe('space')).toBe('autre')
    expect(sectionDe('join')).toBe('autre')
    expect(sectionDe('constructor')).toBe('autre')
  })

  it('tient compte de l\'écran affiché hors de la messagerie', () => {
    expect(sectionOuverte('ready', 'musique')).toBe('sillon')
    expect(sectionOuverte('sections', 'films')).toBe('entracte')
    expect(sectionOuverte('admin', 'home')).toBe('admin')
    expect(sectionOuverte('activate', 'home')).toBe('autre')
    expect(sectionOuverte('denied', 'home')).toBe('autre')
  })

  it('ne compte une session que connectée', () => {
    for (const p of ['ready', 'sections', 'admin', 'activate', 'denied'] as const) expect(sessionOuverte(p)).toBe(true)
    for (const p of ['loading', 'login', 'error'] as const) expect(sessionOuverte(p)).toBe(false)
  })
})
