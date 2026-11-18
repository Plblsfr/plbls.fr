import { fireEvent, render } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { entracteApi } from '../../lib/api'
import { confirmation } from '../../lib/confirm.svelte'
import EntracteAdmin from './EntracteAdmin.svelte'
import type { IntroAdmin, IntroSeason } from './entracte'

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

async function ouvrir(reponse: IntroAdmin = etat()) {
  vi.spyOn(entracteApi, 'adminRequests').mockResolvedValue({ items: [] })
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
