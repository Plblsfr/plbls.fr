import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { app } from '../../lib/store.svelte'
import { AUJOURDHUI, entracte, fauxClient, messagerie, statsPleines, statsVides, usage } from './essais'
import SablierJeu from './SablierJeu.svelte'
import SablierNav from './SablierNav.svelte'
import SablierSpace from './SablierSpace.svelte'
import { sablierApi, type ClientSablier } from './sablier'
import { sablier } from './sablier.svelte'

function brancher(over: Partial<ClientSablier> = {}) {
  const client = fauxClient(over)
  for (const cle of Object.keys(client) as (keyof ClientSablier)[]) vi.spyOn(sablierApi, cle).mockImplementation(client[cle] as never)
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date(`${AUJOURDHUI}T10:00:00`))
  sablier.reinitialiser()
  app.details = { s1: { server: { id: 's1', name: 'Amis', owner_id: 'me', icon_version: 0 }, channels: [{ id: 'c1', name: 'général', kind: 'text', position: 0 }, { id: 'v1', name: 'Canapé', kind: 'voice', position: 1 }], members: [] } }
})
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); sablier.reinitialiser(); app.details = {}; app.sablierMe = null; history.replaceState(null, '', '#/') })

const sansAnnonce = () => ({ unlocked: [], in_progress: [], hidden_remaining: 0 })

describe('SablierJeu', () => {
  it('C1 : la section s’ouvre sur le mode jeu et trophées', async () => {
    brancher({ trophees: async () => ({ ...(await fauxClient().trophees()), unlocked: (await fauxClient().trophees()).unlocked.map((t) => ({ ...t, seen: true })) }) })
    const vue = render(SablierSpace, { onmenu: () => {} })
    expect(await vue.findByRole('heading', { level: 1, name: 'Ton mois de novembre' })).toBeInTheDocument()
    const tro = within(vue.getByRole('region', { name: 'Trophées' }))
    expect(tro.getByText('Débloqués en novembre')).toBeInTheDocument()
    expect(tro.getByText('Encore un épisode', { selector: '[data-trophee="encore_un_episode"] .sab-h3' })).toBeInTheDocument()
    expect(tro.getByRole('progressbar', { name: 'Cinéphile, palier 2' })).toHaveAttribute('aria-valuenow', '76')
    expect(tro.getByText('4 trophées cachés')).toBeInTheDocument()
    expect(tro.getByText('Caché, trouvé')).toBeInTheDocument()
    const series = within(vue.getByRole('region', { name: 'Séries de jours' }))
    expect(series.getByText('Toutes activités')).toBeInTheDocument()
    expect(series.getByText('Pas de série en cours. Record : 9 jours.')).toBeInTheDocument()
    expect(series.queryByText('Sillon')).toBeNull()
    expect(vue.getByRole('button', { name: 'Mois', pressed: true })).toBeInTheDocument()
    expect(vue.getByRole('button', { name: 'Mois suivant' })).toBeDisabled()
    expect(vue.getByRole('button', { name: 'Mois précédent' })).toBeEnabled()
  })

  it('C2 : une stat nulle n’est pas rendue, un domaine sans accès n’a que sa carte courte', async () => {
    const s = statsPleines()
    brancher({ trophees: async () => sansAnnonce(), sillon: async () => ({ access: false }), entracte: async () => s.entracte })
    const vue = render(SablierJeu)
    const domaines = within(await vue.findByRole('region', { name: 'Stats par domaine' }))
    expect(domaines.getByText('Plus longue session')).toBeInTheDocument()
    expect(domaines.queryByText('Intros passées')).toBeNull()
    const refus = vue.container.querySelector('[data-domaine="sillon"]')!
    expect(refus.getAttribute('data-etat')).toBe('sans-acces')
    expect(refus.textContent).toContain('Pas d’accès à Sillon')
    expect(refus.textContent).toContain('Ton compte n’a pas encore accès à la musique. Demande la permission « Sillon » à un administrateur de plbls.fr.')
    expect(refus.textContent).not.toContain('d’écoute')
    expect(domaines.queryByText('Titres écoutés')).toBeNull()
    expect(domaines.getByText('salon supprimé')).toBeInTheDocument()
    expect(domaines.getByText('# général')).toBeInTheDocument()
  })

  it('C2 : un domaine sans donnée a une carte courte qui renvoie vers une période qui en a', async () => {
    const pleines = statsPleines()
    const lu: string[] = []
    brancher({
      trophees: async () => sansAnnonce(),
      usage: async (p) => { lu.push(p); return pleines.usage },
      entracte: async (p) => (p === 'week' ? entracte() : pleines.entracte),
    })
    const vue = render(SablierJeu)
    await vue.findByRole('region', { name: 'Stats par domaine' })
    await fireEvent.click(vue.getByRole('button', { name: 'Semaine' }))
    const carte = await waitFor(() => { const c = vue.container.querySelector('[data-domaine="entracte"][data-etat="vide"]'); expect(c).not.toBeNull(); return c as HTMLElement })
    expect(carte.textContent).toContain('Rien regardé cette semaine')
    await fireEvent.click(await within(carte).findByRole('button', { name: 'Voir novembre' }))
    await vue.findByRole('heading', { level: 1, name: 'Ton mois de novembre' })
    await waitFor(() => expect(vue.container.querySelector('[data-domaine="entracte"][data-etat="pleine"]')).not.toBeNull())
  })

  it('C3 : changer de période et naviguer rechargent les quatre domaines', async () => {
    const lus: string[] = []
    const s = statsPleines()
    brancher({
      trophees: async () => sansAnnonce(),
      usage: async (p, d) => { lus.push(`usage ${p} ${d}`); return s.usage },
      entracte: async (p, d) => { lus.push(`entracte ${p} ${d}`); return s.entracte },
      sillon: async (p, d) => { lus.push(`sillon ${p} ${d}`); return s.sillon },
      messagerie: async (p, d) => { lus.push(`messaging ${p} ${d}`); return s.messaging },
    })
    const vue = render(SablierJeu)
    await vue.findByRole('region', { name: 'Stats par domaine' })
    lus.length = 0
    await fireEvent.click(vue.getByRole('button', { name: 'Année' }))
    await waitFor(() => expect(lus).toHaveLength(4))
    expect(lus.map((l) => l.split(' ')[0]).sort()).toEqual(['entracte', 'messaging', 'sillon', 'usage'])
    expect(new Set(lus.map((l) => l.slice(l.indexOf(' ') + 1)))).toEqual(new Set(['year 2026-11-18']))
    expect(await vue.findByRole('heading', { level: 1, name: 'Ton année 2026' })).toBeInTheDocument()
    expect(vue.getByRole('button', { name: 'Année précédente' })).toBeDisabled()
    expect(vue.getByRole('button', { name: 'Année suivante' })).toBeDisabled()
    await fireEvent.click(vue.getByRole('button', { name: 'Mois' }))
    await fireEvent.click(await vue.findByRole('button', { name: 'Mois précédent' }))
    await fireEvent.click(vue.getByRole('button', { name: 'Mois précédent' }))
    expect(await vue.findByText('Septembre 2026')).toBeInTheDocument()
    expect(vue.getByRole('button', { name: 'Mois précédent' })).toBeDisabled()
    expect(lus.at(-1)).toMatch(/ month 2026-09-30$/)
    await fireEvent.click(vue.getByRole('button', { name: 'Tout' }))
    await waitFor(() => expect(vue.queryByRole('button', { name: 'Mois précédent' })).toBeNull())
  })

  it('personne sans activité : un état vide, aucune carte à zéro', async () => {
    const v = statsVides(null)
    brancher({ usage: async () => v.usage, entracte: async () => v.entracte, sillon: async () => v.sillon, messagerie: async () => v.messaging, trophees: async () => ({ unlocked: [], in_progress: [{ id: 21, key: 'premier_jour', name: 'Premier jour', description: '', icon: 'etoile', tier: 0, tiers: 1, value: 0, next_threshold: 1, unit: 'count' }], hidden_remaining: 2 }), series: async () => ({ global: { current: 0, best: 0 }, entracte: null, sillon: null, messaging: { current: 0, best: 0 } }) })
    const vue = render(SablierJeu)
    expect(await vue.findByText('Rien à raconter pour l’instant')).toBeInTheDocument()
    expect(vue.getByText('3 trophées à débloquer')).toBeInTheDocument()
    expect(vue.queryByRole('region', { name: 'Stats par domaine' })).toBeNull()
  })

  it('Sablier injoignable : état d’erreur avec « Réessayer »', async () => {
    let panne = true
    brancher({ trophees: async () => { if (panne) throw new TypeError('Failed to fetch'); return sansAnnonce() } })
    const vue = render(SablierJeu)
    expect(await vue.findByRole('alert')).toHaveTextContent('Les stats sont injoignables pour le moment.')
    panne = false
    await fireEvent.click(vue.getByRole('button', { name: 'Réessayer' }))
    expect(await vue.findByRole('region', { name: 'Stats par domaine' })).toBeInTheDocument()
  })

  it('C5 : l’annonce d’un trophée non vu le marque vu, propose de l’afficher, puis la pastille s’éteint', async () => {
    const vus = new Set<number>()
    const partager = vi.fn(async () => ({ stats: [], trophies: [{ id: 2, name: 'Encore un épisode', icon: 'tele', shared: true }] }))
    const marquer = vi.fn(async (v: { trophies?: number[] | 'all' }) => { if (Array.isArray(v.trophies)) for (const id of v.trophies) vus.add(id) })
    brancher({ nonVus: async () => ({ trophies: vus.has(2) ? 0 : 1, recaps: 0 }), marquerVus: marquer, partager, recaps: async () => [] })
    await sablier.chargerNonVus()
    const nav = render(SablierNav)
    expect(nav.getByRole('img', { name: 'Nouveau trophée' })).toBeInTheDocument()
    const vue = render(SablierJeu)
    const annonce = await waitFor(() => { const a = vue.container.ownerDocument.querySelector('[data-annonce="trophee"]'); expect(a).not.toBeNull(); return a as HTMLElement })
    expect(annonce.textContent).toContain('Encore un épisode')
    expect(annonce.textContent).toContain('Privé tant que tu ne l’affiches pas sur ton profil.')
    expect(annonce.textContent).toContain('Prochain palier à 1 000.')
    await fireEvent.click(within(annonce).getByRole('button', { name: 'L’afficher sur mon profil' }))
    await waitFor(() => expect(partager).toHaveBeenCalledWith({ trophies: { 2: true } }))
    await waitFor(() => expect(annonce.textContent).toContain('Affiché sur ton profil.'))
    await fireEvent.click(within(annonce).getByRole('button', { name: 'Fermer' }))
    await waitFor(() => expect(marquer).toHaveBeenCalledWith({ trophies: [2] }))
    await waitFor(() => expect(nav.queryByRole('img', { name: 'Nouveau trophée' })).toBeNull())
    expect(vue.container.ownerDocument.querySelector('[data-annonce="trophee"]')).toBeNull()
  })

  it('navigation : « Administration » seulement avec sablier:admin', () => {
    app.sablierMe = { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: true, admin: false }, sections: ['stats'] }
    const sans = render(SablierNav)
    expect(sans.getAllByRole('link').map((a) => a.textContent?.trim())).toEqual(['Jeu et trophées', 'Récaps', 'Partage et données'])
    sans.unmount()
    app.sablierMe = { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: true, admin: true }, sections: ['stats', 'admin'] }
    const avec = render(SablierNav)
    expect(avec.getByRole('link', { name: 'Administration' })).toHaveAttribute('href', '#/stats/admin')
  })

  it('utilisation seule, sans message : pas de ligne de messagerie à zéro', async () => {
    brancher({ trophees: async () => sansAnnonce(), usage: async () => usage({ seconds: 600, active_days: 1, by_day: [{ day: AUJOURDHUI, seconds: 600 }] }), entracte: async () => entracte(), sillon: async () => ({ access: false }), messagerie: async () => messagerie() })
    const vue = render(SablierJeu)
    await vue.findByRole('region', { name: 'Stats par domaine' })
    expect(vue.container.querySelector('[data-domaine="messaging"]')?.getAttribute('data-etat')).toBe('vide')
    expect(vue.queryByText('Réactions')).toBeNull()
    expect(vue.queryByText('0')).toBeNull()
  })
})
