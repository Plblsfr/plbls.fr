import { fireEvent, render } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { entracteApi } from '../../lib/api'
import { confirmation } from '../../lib/confirm.svelte'
import EntracteAdmin from './EntracteAdmin.svelte'
import type { EntracteRequest, EntracteStatus, IntroAdmin, IntroSeason } from './entracte'

const saison = (over: Partial<IntroSeason> = {}): IntroSeason => ({ number: 1, episodes: 13, found: 0, state: 'jamais', error: null, updated_at: null, ...over })
const etat = (over: Partial<IntroAdmin> = {}): IntroAdmin => ({
  mode: 'manuel', available: true,
  items: [
    { id: 'a', title: 'Les Veilleurs', year: 2019, seasons: [saison(), saison({ number: 2, episodes: 1 })] },
    { id: 'b', title: 'La Combe', year: null, seasons: [saison({ state: 'trouvee', found: 12, updated_at: '2026-11-01T10:00:00.000Z' }), saison({ number: 2, state: 'erreur', error: 'empreinte audio impossible pour 3 épisode(s) sur 3' })] },
    { id: 'c', title: 'Les Marées', year: 2021, seasons: [saison({ state: 'en_cours' })] },
  ],
  ...over,
})

const sain: EntracteStatus = {
  sync: [{ source: 'radarr', succeeded_at: '2026-11-01T10:00:00.000Z', attempted_at: null, error: null }, { source: 'sonarr', succeeded_at: null, attempted_at: null, error: null }],
  transcoding: { requested: 'auto', active: 'logiciel', message: null, running: 0 },
  configured: { radarr: true, sonarr: true, tmdb: true, push: true },
}

async function ouvrir(reponse: IntroAdmin = etat()) {
  vi.spyOn(entracteApi, 'adminRequests').mockResolvedValue({ items: [] })
  vi.spyOn(entracteApi, 'status').mockResolvedValue(sain)
  const liste = vi.spyOn(entracteApi, 'intros').mockResolvedValue(reponse)
  const vue = render(EntracteAdmin)
  await fireEvent.click(vue.getByRole('button', { name: 'Intros' }))
  await vue.findByText('Les Veilleurs (2019)')
  return { vue, liste }
}
const ligne = (vue: Awaited<ReturnType<typeof ouvrir>>['vue'], titre: string) => vue.getByText(titre).closest('li')!
const boutons = (li: Element) => [...li.querySelectorAll('button')].map((b) => b.textContent?.trim())

describe('EntracteAdmin : intros', () => {
  afterEach(() => { confirmation.en?.resoudre({ ok: false, valeur: '' }); vi.restoreAllMocks() })

  it('affiche le mode, les séries, leurs saisons et leurs états', async () => {
    const { vue } = await ouvrir()
    const mode = vue.getByRole('switch', { name: /Détection automatique/ }) as HTMLInputElement
    expect(mode.checked).toBe(false)
    const veilleurs = ligne(vue, 'Les Veilleurs (2019)')
    expect(veilleurs.textContent).toContain('Saison 1 · 13 épisodes')
    expect(veilleurs.textContent).toContain('Saison 2 · 1 épisode')
    expect(veilleurs.textContent).toContain('Jamais analysée')
    expect(boutons(veilleurs)).toEqual(['Analyser'])
    const combe = ligne(vue, 'La Combe')
    expect(combe.textContent).toContain('Intro trouvée (12 sur 13)')
    expect(combe.querySelector('[role="alert"]')!.textContent).toContain('empreinte audio impossible pour 3 épisode(s) sur 3')
    expect(boutons(combe)).toEqual(['Réanalyser', 'Effacer'])
    const marees = ligne(vue, 'Les Marées (2021)')
    expect(marees.textContent).toContain('Analyse en cours')
    expect((marees.querySelector('button') as HTMLButtonElement).disabled).toBe(true)
    expect(vue.queryByText(/Chromaprint est indisponible/)).toBeNull()
  })

  it('l’interrupteur change le mode puis recharge la liste', async () => {
    const { vue, liste } = await ouvrir()
    const regler = vi.spyOn(entracteApi, 'introMode').mockResolvedValue({ mode: 'automatique' })
    liste.mockResolvedValue(etat({ mode: 'automatique' }))
    await fireEvent.click(vue.getByRole('switch', { name: /Détection automatique/ }))
    expect(regler).toHaveBeenCalledWith('automatique')
    await vi.waitFor(() => expect(liste).toHaveBeenCalledTimes(2))
    await vi.waitFor(() => expect((vue.getByRole('switch', { name: /Détection automatique/ }) as HTMLInputElement).checked).toBe(true))
    await fireEvent.click(vue.getByRole('switch', { name: /Détection automatique/ }))
    expect(regler).toHaveBeenLastCalledWith('manuel')
  })

  it('« Analyser » met la série en file et « Réanalyser » la relance', async () => {
    const { vue, liste } = await ouvrir()
    const analyser = vi.spyOn(entracteApi, 'introAnalyze').mockResolvedValue({ queued: 2 })
    await fireEvent.click(ligne(vue, 'Les Veilleurs (2019)').querySelector('button')!)
    expect(analyser).toHaveBeenCalledWith('a')
    await vi.waitFor(() => expect(liste).toHaveBeenCalledTimes(2))
    await fireEvent.click(ligne(vue, 'La Combe').querySelector('button')!)
    expect(analyser).toHaveBeenLastCalledWith('b')
  })

  it('« Effacer » demande une confirmation avant d’effacer', async () => {
    const { vue, liste } = await ouvrir()
    const effacer = vi.spyOn(entracteApi, 'introClear').mockResolvedValue(undefined)
    await fireEvent.click(vue.getByRole('button', { name: 'Effacer les intros de La Combe' }))
    await vi.waitFor(() => expect(confirmation.en?.titre).toBe('Effacer les intros de « La Combe »'))
    confirmation.en!.resoudre({ ok: false, valeur: '' })
    await Promise.resolve()
    expect(effacer).not.toHaveBeenCalled()
    await fireEvent.click(vue.getByRole('button', { name: 'Effacer les intros de La Combe' }))
    await vi.waitFor(() => expect(confirmation.en).toBeTruthy())
    confirmation.en!.resoudre({ ok: true, valeur: '' })
    await vi.waitFor(() => expect(effacer).toHaveBeenCalledWith('b'))
    await vi.waitFor(() => expect(liste).toHaveBeenCalledTimes(2))
  })

  it('prévient quand Chromaprint manque et quand il n’y a aucune série', async () => {
    vi.spyOn(entracteApi, 'adminRequests').mockResolvedValue({ items: [] })
    vi.spyOn(entracteApi, 'intros').mockResolvedValue({ mode: 'automatique', available: false, items: [] })
    const vue = render(EntracteAdmin)
    await fireEvent.click(vue.getByRole('button', { name: 'Intros' }))
    expect((await vue.findByRole('alert')).textContent).toContain('Chromaprint est indisponible')
    expect(vue.getByText('Aucune série dans la bibliothèque.')).toBeTruthy()
    expect((vue.getByRole('switch', { name: /Détection automatique/ }) as HTMLInputElement).checked).toBe(true)
  })
})

describe('EntracteAdmin : état et demandes', () => {
  afterEach(() => { vi.restoreAllMocks() })
  const demande = (over: Partial<EntracteRequest>): EntracteRequest => ({ id: 'd1', kind: 'movie', tmdb_id: 1, seasons: null, title: 'Le Règne', year: 2023, poster_url: null, state: 'en_attente', reason: null, created_at: '2026-11-15T10:00:00.000Z', updated_at: '2026-11-15T10:00:00.000Z', requesters: [{ name: 'chloe', joined_at: '' }], ...over })

  it('l’état des services est visible sans changer d’onglet, erreur comprise', async () => {
    vi.spyOn(entracteApi, 'adminRequests').mockResolvedValue({ items: [] })
    vi.spyOn(entracteApi, 'status').mockResolvedValue({ ...sain, sync: [sain.sync[0], { source: 'sonarr', succeeded_at: null, attempted_at: null, error: 'connexion refusée' }] })
    const vue = render(EntracteAdmin)
    const etatServices = await vue.findByRole('region', { name: 'État des services' })
    await vi.waitFor(() => expect(etatServices.textContent).toContain('Radarr'))
    expect(etatServices.textContent).toContain('Joignable')
    expect(etatServices.textContent).toContain('connexion refusée')
    expect(etatServices.textContent).toContain('Transcodage')
    expect(vue.queryByRole('button', { name: 'État' })).toBeNull()
  })

  it('un envoi en échec remonte au-dessus des demandes avec « Renvoyer », sans doublon dans la liste', async () => {
    const echec = demande({ id: 'e1', title: 'En panne', state: 'acceptee', sent: false, send_error: 'dossier racine introuvable' })
    vi.spyOn(entracteApi, 'status').mockResolvedValue(sain)
    vi.spyOn(entracteApi, 'adminRequests').mockImplementation(async (f) => ({ items: f.state === 'acceptee' ? [echec] : f.state === 'en_attente' ? [demande({})] : [echec, demande({})] }))
    const vue = render(EntracteAdmin)
    await vue.findByRole('heading', { name: 'Envoi en échec' })
    const ligneEchec = vue.getByText('En panne (2023)').closest('li')!
    expect(ligneEchec.textContent).toContain('dossier racine introuvable')
    expect([...ligneEchec.querySelectorAll('button')].map((b) => b.textContent?.trim())).toEqual(['Refuser', 'Renvoyer'])
    await fireEvent.click(vue.getByRole('button', { name: 'Toutes' }))
    await vi.waitFor(() => expect(vue.getAllByText('En panne (2023)')).toHaveLength(1))
    expect(vue.getByText('Le Règne (2023)')).toBeTruthy()
  })
})
