import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { partage } from './essais'
import SablierPartage from './SablierPartage.svelte'
import SablierProfil from './SablierProfil.svelte'
import { sablierApi, type ChoixPartage, type Partage, type Profil } from './sablier'
import { sablier } from './sablier.svelte'

beforeEach(() => { sablier.reinitialiser() })
afterEach(() => { vi.restoreAllMocks(); sablier.reinitialiser() })

function serveur() {
  const etat: Partage = partage()
  const VALEURS: Record<string, { unit: 'count' | 'seconds'; mois: number; total: number }> = { 'messaging.messages': { unit: 'count', mois: 642, total: 8640 }, 'usage.active_seconds': { unit: 'seconds', mois: 112_800, total: 1_483_200 }, 'entracte.movies_watched': { unit: 'count', mois: 4, total: 38 } }
  const profil = (): Profil => {
    const stats = etat.stats.filter((s) => s.shared)
    return {
      month: stats.map((s) => ({ key: s.key, label: s.label, domain: s.domain, unit: VALEURS[s.key].unit, value: VALEURS[s.key].mois })),
      total: stats.map((s) => ({ key: s.key, label: s.label, domain: s.domain, unit: VALEURS[s.key].unit, value: VALEURS[s.key].total })),
      trophies: etat.trophies.filter((t) => t.shared).map((t) => ({ name: t.name, description: '', icon: t.icon, tier: 2, unlocked_at: '2026-11-14' })),
    }
  }
  const partager = vi.spyOn(sablierApi, 'partager').mockImplementation(async (choix: ChoixPartage) => {
    for (const s of etat.stats) if (choix.stats && s.key in choix.stats) s.shared = choix.stats[s.key]
    for (const t of etat.trophies) if (choix.trophies && String(t.id) in choix.trophies) t.shared = choix.trophies[String(t.id)]
    return structuredClone(etat)
  })
  vi.spyOn(sablierApi, 'partage').mockImplementation(async () => structuredClone(etat))
  const lireProfil = vi.spyOn(sablierApi, 'profil').mockImplementation(async () => profil())
  return { partager, lireProfil }
}
const bloc = (vue: { container: HTMLElement }) => vue.container.querySelector('[data-bloc="stats-profil"]')

describe('partage sur le profil', () => {
  it('C6 : rien n’est public par défaut, et le profil n’a alors pas de bloc', async () => {
    const { lireProfil } = serveur()
    const vue = render(SablierPartage)
    const reglages = await vue.findAllByRole('switch')
    expect(reglages).toHaveLength(4)
    for (const r of reglages) expect(r).not.toBeChecked()
    expect(vue.getByText('0 affiché sur 1 débloqué. Un nouveau trophée est privé.')).toBeInTheDocument()
    expect(vue.getByText('Vu seulement par qui a aussi accès à Entracte.')).toBeInTheDocument()
    const profil = render(SablierProfil, { sub: 'kc-moi' })
    await waitFor(() => expect(lireProfil).toHaveBeenCalledWith('kc-moi'))
    await waitFor(() => expect(lireProfil.mock.results[0].type).toBe('return'))
    expect(bloc(profil)).toBeNull()
  })

  it('C6 : une stat rendue publique part en PUT, apparaît sur le profil vu par une autre personne, puis disparaît repassée en privé', async () => {
    const { partager, lireProfil } = serveur()
    const vue = render(SablierPartage)
    const messages = (await vue.findByRole('switch', { name: 'Messages envoyés' })) as HTMLInputElement
    await fireEvent.click(messages)
    await waitFor(() => expect(partager).toHaveBeenCalledWith({ stats: { 'messaging.messages': true } }))
    await waitFor(() => expect(sablier.partage?.stats.find((s) => s.key === 'messaging.messages')?.shared).toBe(true))

    const autre = render(SablierProfil, { sub: 'kc-moi' })
    await waitFor(() => expect(bloc(autre)).not.toBeNull())
    expect(lireProfil).toHaveBeenCalledWith('kc-moi')
    expect(bloc(autre)!.textContent).toContain('Messages envoyés')
    expect(bloc(autre)!.textContent).toContain('642')
    expect(bloc(autre)!.textContent).not.toContain('Temps actif')
    await fireEvent.click(autre.getByRole('button', { name: 'Total' }))
    expect(bloc(autre)!.textContent).toContain('8 640')
    expect(autre.queryByRole('button', { name: 'Régler ce que j’affiche' })).toBeNull()
    autre.unmount()

    await fireEvent.click(vue.getByRole('switch', { name: 'Messages envoyés' }))
    await waitFor(() => expect(partager).toHaveBeenLastCalledWith({ stats: { 'messaging.messages': false } }))
    const apres = render(SablierProfil, { sub: 'kc-moi' })
    await waitFor(() => expect(lireProfil.mock.results.at(-1)?.type).toBe('return'))
    await waitFor(() => expect(lireProfil).toHaveBeenCalledTimes(2))
    expect(bloc(apres)).toBeNull()
  })

  it('C6 : un trophée se partage par son identifiant et paraît sur le profil', async () => {
    const { partager } = serveur()
    const vue = render(SablierPartage)
    await fireEvent.click(await vue.findByRole('switch', { name: 'Encore un épisode' }))
    await waitFor(() => expect(partager).toHaveBeenCalledWith({ trophies: { 2: true } }))
    expect(await vue.findByText('1 affiché sur 1 débloqué. Un nouveau trophée est privé.')).toBeInTheDocument()
    const profil = render(SablierProfil, { sub: 'kc-moi', mien: true, onregler: () => {} })
    await waitFor(() => expect(bloc(profil)).not.toBeNull())
    expect(bloc(profil)!.textContent).toContain('Encore un épisode')
    expect(bloc(profil)!.textContent).toContain('Palier 2 · 14 nov. 2026')
    expect(profil.getByRole('button', { name: 'Régler ce que j’affiche' })).toBeInTheDocument()
  })

  it('un refus du serveur remet le réglage et le dit', async () => {
    serveur()
    vi.spyOn(sablierApi, 'partager').mockRejectedValue(new ApiError(400, 'invalid', 'stat non partageable'))
    const vue = render(SablierPartage)
    const r = (await vue.findByRole('switch', { name: 'Temps actif' })) as HTMLInputElement
    await fireEvent.click(r)
    await waitFor(() => expect(r.checked).toBe(false))
    await waitFor(() => expect(r).toBeEnabled())
  })

  it('le bloc du profil reste absent si Sablier échoue', async () => {
    const lire = vi.spyOn(sablierApi, 'profil').mockRejectedValue(new TypeError('Failed to fetch'))
    const profil = render(SablierProfil, { sub: 'kc-autre' })
    await waitFor(() => expect(lire.mock.results[0]?.type).toBe('return'))
    await expect(lire.mock.results[0].value).rejects.toBeInstanceOf(TypeError)
    expect(bloc(profil)).toBeNull()
  })

  it('profil d’une autre personne : stats et trophées servis, libellés du domaine', async () => {
    vi.spyOn(sablierApi, 'profil').mockResolvedValue({ month: [], total: [{ key: 'entracte.movies_watched', label: 'Films vus', domain: 'entracte', unit: 'count', value: 38 }], first_day: '2024-03-03', trophies: [{ name: 'Signature', description: '', icon: 'icone-inconnue', tier: 1, unlocked_at: '2026-10-02' }] })
    const profil = render(SablierProfil, { sub: 'kc-camille' })
    await waitFor(() => expect(bloc(profil)).not.toBeNull())
    expect(bloc(profil)!.textContent).toContain('Films vus · Entracte')
    expect(bloc(profil)!.textContent).toContain('Depuis le 3 mars 2024.')
    expect(bloc(profil)!.textContent).toContain('Signature')
    expect(profil.queryByRole('group', { name: 'Période des stats' })).toBeNull()
  })
})
