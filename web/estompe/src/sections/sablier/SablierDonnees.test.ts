import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ConfirmModal from '../../components/ConfirmModal.svelte'
import { ApiError } from '../../lib/api'
import { confirmation } from '../../lib/confirm.svelte'
import { router } from '../../lib/route.svelte'
import SablierDonnees from './SablierDonnees.svelte'
import { sablierApi, type Vue } from './sablier'
import { sablier } from './sablier.svelte'

let enregistres: { nom: string; blob: Blob }[]
let dernier: Blob | null

beforeEach(() => {
  enregistres = []
  dernier = null
  sablier.reinitialiser()
  vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: vi.fn((b: Blob) => { dernier = b; return 'blob:essai' }), revokeObjectURL: vi.fn() }))
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { if (dernier) enregistres.push({ nom: this.download, blob: dernier }) })
  vi.spyOn(sablierApi, 'nonVus').mockResolvedValue({ trophies: 0, recaps: 0 })
})
afterEach(() => { confirmation.en?.resoudre({ ok: false, valeur: '' }); vi.restoreAllMocks(); sablier.reinitialiser(); history.replaceState(null, '', '#/') })

const monter = (vues: Vue[] = []) => {
  vi.spyOn(sablierApi, 'vues').mockResolvedValue(vues)
  render(ConfirmModal)
  return render(SablierDonnees)
}

describe('SablierDonnees', () => {
  it('export : enregistre le fichier servi sous le nom annoncé', async () => {
    const exporter = vi.spyOn(sablierApi, 'exporter').mockResolvedValue({ blob: new Blob(['{"sub":"kc-moi"}'], { type: 'application/json' }), nom: 'sablier-export-2026-11-18.json' })
    const vue = monter()
    await fireEvent.click(vue.getByRole('button', { name: 'Télécharger mes données' }))
    await waitFor(() => expect(enregistres).toHaveLength(1))
    expect(exporter).toHaveBeenCalledTimes(1)
    expect(enregistres[0].nom).toBe('sablier-export-2026-11-18.json')
    expect(await enregistres[0].blob.text()).toBe('{"sub":"kc-moi"}')
    expect(vue.getByRole('button', { name: 'Télécharger mes données' })).toBeEnabled()
  })

  it('export : indique l’attente, puis un échec lisible et une relance', async () => {
    let finir!: (e: Error) => void
    vi.spyOn(sablierApi, 'exporter').mockImplementation(() => new Promise((_ok, non) => { finir = non }))
    const vue = monter()
    await fireEvent.click(vue.getByRole('button', { name: 'Télécharger mes données' }))
    expect(vue.getByRole('button', { name: 'Préparation du fichier…' })).toBeDisabled()
    finir(new TypeError('Failed to fetch'))
    expect(await vue.findByRole('alert')).toHaveTextContent('L’export a échoué. Les stats sont injoignables pour le moment.')
    expect(vue.getByRole('button', { name: 'Réessayer l’export' })).toBeEnabled()
    expect(enregistres).toEqual([])
  })

  it('C2 : le journal affiche la date et le nom de l’admin ; sans consultation, il est absent', async () => {
    const vue = monter([{ date: new Date(2026, 10, 12, 21, 14).toISOString(), admin_name: 'Sacha', action: 'consultation' }, { date: new Date(2026, 8, 3, 10, 2).toISOString(), admin_name: 'Inès', action: 'effacement' }])
    const journal = within(await vue.findByRole('region', { name: 'Journal des consultations' }))
    const lignes = journal.getAllByRole('listitem').map((li) => li.textContent?.replace(/\s+/g, ' ').trim())
    expect(lignes).toEqual(['SachaConsultation12 nov. 2026 à 21:14', 'InèsEffacement des données3 sept. 2026 à 10:02'])
    vue.unmount()
    const lire = vi.spyOn(sablierApi, 'vues').mockResolvedValue([])
    const vide = render(SablierDonnees)
    await waitFor(() => expect(lire.mock.results[0]?.type).toBe('return'))
    await lire.mock.results[0].value
    expect(vide.queryByRole('region', { name: 'Journal des consultations' })).toBeNull()
    expect(vide.getByRole('region', { name: 'Mes données' })).toBeInTheDocument()
  })

  it('C4 : l’effacement n’active le bouton qu’avec le mot EFFACER exact, puis efface et remet la section à zéro', async () => {
    const effacer = vi.spyOn(sablierApi, 'effacer').mockResolvedValue(undefined)
    sablier.nonVus = { trophies: 2, recaps: 1 }
    sablier.charge = true
    history.replaceState(null, '', '#/stats/partage')
    const vue = monter()
    await fireEvent.click(vue.getByRole('button', { name: 'Effacer mes stats…' }))
    const boite = within(await waitFor(() => { const d = document.querySelector<HTMLElement>('[role="alertdialog"]'); expect(d).not.toBeNull(); return d! }))
    expect(boite.getByText(/le journal des consultations est conservé/)).toBeInTheDocument()
    const valider = boite.getByRole('button', { name: 'Effacer définitivement' })
    const champ = boite.getByLabelText('Écris EFFACER pour confirmer')
    expect(valider).toBeDisabled()
    for (const essai of ['effacer', 'EFFACE', 'EFFACER !']) {
      await fireEvent.input(champ, { target: { value: essai } })
      expect(valider).toBeDisabled()
    }
    expect(effacer).not.toHaveBeenCalled()
    await fireEvent.input(champ, { target: { value: 'EFFACER' } })
    expect(valider).toBeEnabled()
    await fireEvent.click(valider)
    await waitFor(() => expect(effacer).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(router.current).toMatchObject({ name: 'stats', section: undefined }))
    expect(sablier.charge).toBe(false)
    expect(sablier.stats).toBeNull()
    await waitFor(() => expect(sablier.nonVus).toEqual({ trophies: 0, recaps: 0 }))
  })

  it('effacement annulé ou refusé : rien n’est remis à zéro', async () => {
    const effacer = vi.spyOn(sablierApi, 'effacer').mockRejectedValue(new ApiError(503, 'aegis_unavailable', 'service de droits injoignable'))
    sablier.charge = true
    const vue = monter()
    await fireEvent.click(vue.getByRole('button', { name: 'Effacer mes stats…' }))
    await waitFor(() => expect(confirmation.en).not.toBeNull())
    await fireEvent.click(within(document.querySelector<HTMLElement>('[role="alertdialog"]')!).getByRole('button', { name: 'Annuler' }))
    await waitFor(() => expect(confirmation.en).toBeNull())
    expect(effacer).not.toHaveBeenCalled()
    await fireEvent.click(vue.getByRole('button', { name: 'Effacer mes stats…' }))
    await waitFor(() => expect(confirmation.en).not.toBeNull())
    confirmation.en!.resoudre({ ok: true, valeur: 'EFFACER' })
    await waitFor(() => expect(effacer).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(vue.getByRole('button', { name: 'Effacer mes stats…' })).toBeEnabled())
    expect(sablier.charge).toBe(true)
  })
})
