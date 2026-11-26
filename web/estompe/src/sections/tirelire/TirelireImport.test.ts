import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ConfirmModal from '../../components/ConfirmModal.svelte'
import { ApiError } from '../../lib/api'
import { confirmation } from '../../lib/confirm.svelte'
import { dejaImporte, importe, pdf, refuse, releve } from './essais'
import TirelireImport from './TirelireImport.svelte'
import { FICHIER_MAX, tirelireApi, type Releve, type ResultatImport } from './tirelire'
import { tirelire } from './tirelire.svelte'

const texte = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? ''

beforeEach(() => tirelire.reinitialiser())
afterEach(() => { confirmation.en?.resoudre({ ok: false, valeur: '' }); vi.restoreAllMocks(); tirelire.reinitialiser() })

async function monter(releves: Releve[] = []) {
  const lire = vi.spyOn(tirelireApi, 'releves').mockImplementation(async () => [...releves])
  await tirelire.ouvrir()
  render(ConfirmModal)
  return { vue: render(TirelireImport), lire }
}

async function choisir(vue: { getByLabelText: (nom: string) => HTMLElement }, fichiers: File[]) {
  const champ = vue.getByLabelText('Relevés PDF à importer') as HTMLInputElement
  Object.defineProperty(champ, 'files', { value: fichiers, configurable: true })
  await fireEvent.change(champ)
}

describe('TirelireImport', () => {
  it('envoie les PDF choisis et affiche les trois issues, refus en tête', async () => {
    const resultats: ResultatImport[] = [importe('releve-novembre.pdf'), dejaImporte('releve-octobre.pdf', { id: 10, number: 10, imported_at: new Date(2026, 9, 2, 10, 0).toISOString() }), refuse('releve-aout.pdf')]
    const importer = vi.spyOn(tirelireApi, 'importer').mockResolvedValue(resultats)
    const { vue } = await monter()
    await choisir(vue, [pdf('releve-novembre.pdf'), pdf('releve-octobre.pdf'), pdf('releve-aout.pdf')])
    expect(await vue.findByRole('heading', { name: '3 fichiers traités' })).toBeInTheDocument()
    expect(importer.mock.calls[0][0].map((f) => f.name)).toEqual(['releve-novembre.pdf', 'releve-octobre.pdf', 'releve-aout.pdf'])
    expect(within(vue.getByRole('list', { name: 'Bilan de l’import' })).getAllByRole('listitem').map(texte)).toEqual(['1 importé', '1 déjà importé', '1 refusé'])
    expect([...vue.container.querySelectorAll('[data-issue]')].map((s) => s.getAttribute('data-issue'))).toEqual(['rejected', 'imported', 'already_imported'])
    const ok = vue.getByRole('region', { name: 'Fichier importé : releve-novembre.pdf' })
    expect(texte(ok)).toContain('Du 31 oct. au 26 nov. 2026 · 84 opérations')
    expect(within(ok).getByText('totaux vérifiés')).toBeInTheDocument()
    expect(texte(vue.getByRole('region', { name: 'Fichier déjà importé : releve-octobre.pdf' }))).toContain('Déjà importé le 2 oct. 2026 · aucune opération ajoutée')
    expect(vue.getByRole('link', { name: 'Voir les 84 nouvelles opérations' })).toHaveAttribute('href', '#/tirelire/operations')
  })

  it('refus au contrôle : écarts entre totaux lus et totaux du relevé, lignes non reconnues avec leur page', async () => {
    vi.spyOn(tirelireApi, 'importer').mockResolvedValue([refuse('releve-aout.pdf')])
    const { vue } = await monter()
    await choisir(vue, [pdf('releve-aout.pdf')])
    const carte = within(await vue.findByRole('region', { name: 'Fichier refusé : releve-aout.pdf' }))
    expect(texte(carte.getByRole('alert'))).toBe('Le contrôle a échoué : les totaux lus ne correspondent pas aux totaux imprimés sur le relevé et des lignes du relevé n’ont pas été reconnues.')
    expect(carte.getByText('Rien n’a été importé de ce fichier.')).toBeInTheDocument()
    expect(carte.getAllByRole('columnheader').map(texte)).toEqual(['Contrôle', 'Lu', 'Relevé', 'Écart'])
    expect(carte.getAllByRole('row').slice(1).map((r) => [...r.children].map(texte))).toEqual([
      ['Débits', '2 318,45 €', '2 391,25 €', '−72,80 €'],
      ['Solde final', '6 226,80 €', '6 154,00 €', '+72,80 €'],
    ])
    expect(carte.getByText('3 lignes non reconnues')).toBeInTheDocument()
    expect(within(carte.getByRole('list', { name: 'Lignes non reconnues' })).getAllByRole('listitem').map(texte)).toEqual(['page 212/08 CB*0000 RETR.DAB 60,0O', 'page 219/08 VIR INST EXEMPLE 9,90', 'page 3REF 44-A / 2,90'])
    expect(vue.queryByRole('link', { name: /Voir les/ })).toBeNull()
  })

  it('refus simples : le motif est traduit, les éléments introuvables sont listés', async () => {
    vi.spyOn(tirelireApi, 'importer').mockResolvedValue([
      { file: 'photo.pdf', status: 'rejected', error: { code: 'not_a_pdf', message: 'ce fichier n’est pas un PDF' } },
      { file: 'autre-banque.pdf', status: 'rejected', error: { code: 'unknown_format', message: 'format inconnu' } },
      { file: 'tronque.pdf', status: 'rejected', error: { code: 'checks_failed', message: 'relevé refusé', missing: ['total des opérations', 'nouveau solde'], gaps: [], unrecognized_lines: [] } },
    ])
    const { vue } = await monter()
    await choisir(vue, [pdf('photo.pdf'), pdf('autre-banque.pdf'), pdf('tronque.pdf')])
    expect(texte(within(await vue.findByRole('region', { name: 'Fichier refusé : photo.pdf' })).getByRole('alert'))).toBe('Ce fichier n’est pas un PDF.')
    expect(texte(within(vue.getByRole('region', { name: 'Fichier refusé : autre-banque.pdf' })).getByRole('alert'))).toBe('Ce n’est pas un relevé de compte du Crédit Mutuel de Bretagne.')
    const tronque = within(vue.getByRole('region', { name: 'Fichier refusé : tronque.pdf' }))
    expect(texte(tronque.getByRole('alert'))).toBe('Le contrôle a échoué : des éléments du relevé sont introuvables.')
    expect(tronque.getAllByRole('listitem').map(texte)).toEqual(['total des opérations', 'nouveau solde'])
    expect(tronque.queryByRole('table')).toBeNull()
  })

  it('fichier trop gros : refusé sur place, sans envoi', async () => {
    const importer = vi.spyOn(tirelireApi, 'importer').mockResolvedValue([])
    const { vue } = await monter()
    await choisir(vue, [pdf('enorme.pdf', FICHIER_MAX + 1)])
    const carte = within(await vue.findByRole('region', { name: 'Fichier refusé : enorme.pdf' }))
    expect(texte(carte.getByRole('alert'))).toBe('Ce fichier dépasse 8 Mo.')
    expect(carte.queryByRole('button', { name: 'Réessayer' })).toBeNull()
    expect(importer).not.toHaveBeenCalled()
  })

  it('envoi refusé (413, réseau) : fichiers « non envoyés », message lisible, relance', async () => {
    const importer = vi.spyOn(tirelireApi, 'importer').mockRejectedValueOnce(new ApiError(413, 'too_large', 'envoi trop volumineux')).mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValueOnce([importe('a.pdf')])
    const { vue } = await monter()
    await choisir(vue, [pdf('a.pdf')])
    const carte = () => within(vue.getByRole('region', { name: /a\.pdf$/ }))
    await waitFor(() => expect(texte(carte().getByRole('alert'))).toBe('Envoi trop volumineux : 8 Mo au plus par fichier, 32 Mo par envoi.'))
    expect(carte().getByText('Non envoyé')).toBeInTheDocument()
    await fireEvent.click(carte().getByRole('button', { name: 'Réessayer' }))
    await waitFor(() => expect(texte(carte().getByRole('alert'))).toBe('Tirelire est injoignable. Vérifie ta connexion.'))
    await fireEvent.click(carte().getByRole('button', { name: 'Réessayer' }))
    expect(await vue.findByRole('region', { name: 'Fichier importé : a.pdf' })).toBeInTheDocument()
    expect(importer).toHaveBeenCalledTimes(3)
  })

  it('pendant l’envoi, le bouton est inactif et l’attente est annoncée', async () => {
    let finir!: (r: ResultatImport[]) => void
    vi.spyOn(tirelireApi, 'importer').mockImplementation(() => new Promise((ok) => { finir = ok }))
    const { vue } = await monter()
    await choisir(vue, [pdf('a.pdf')])
    expect(await vue.findByRole('button', { name: 'Envoi en cours…' })).toBeDisabled()
    expect(vue.getByRole('status', { name: 'Envoi et contrôle des relevés…' })).toBeInTheDocument()
    finir([importe('a.pdf')])
    expect(await vue.findByRole('button', { name: 'Choisir des fichiers' })).toBeEnabled()
  })

  it('dépôt par glisser-déposer', async () => {
    const importer = vi.spyOn(tirelireApi, 'importer').mockResolvedValue([importe('depose.pdf')])
    const { vue } = await monter()
    await fireEvent.drop(vue.getByRole('group', { name: 'Dépôt de fichiers' }), { dataTransfer: { files: [pdf('depose.pdf')] } })
    expect(await vue.findByRole('region', { name: 'Fichier importé : depose.pdf' })).toBeInTheDocument()
    expect(importer).toHaveBeenCalledTimes(1)
  })

  it('liste les relevés importés avec leur période et leur nombre d’opérations', async () => {
    const { vue } = await monter([releve(), releve({ id: 10, number: 10, from: '2026-09-30', to: '2026-10-31', operations: 1 })])
    const liste = within(vue.getByRole('region', { name: 'Relevés importés' }))
    expect(liste.getAllByRole('listitem').map((li) => texte(li).replace(/Importé le.*$/, '').trim())).toEqual(['Relevé n° 11 de 2026 Du 31 oct. au 26 nov. 2026 · 84 opérations', 'Relevé n° 10 de 2026 Du 30 sept. au 31 oct. 2026 · 1 opération'])
  })

  it('suppression : demande confirmation, ne fait rien sur « Annuler », supprime et relit sur « Supprimer »', async () => {
    const liste = [releve(), releve({ id: 10, number: 10 })]
    const supprimer = vi.spyOn(tirelireApi, 'supprimer').mockImplementation(async (id) => { liste.splice(liste.findIndex((r) => r.id === id), 1) })
    vi.spyOn(tirelireApi, 'releves').mockImplementation(async () => [...liste])
    await tirelire.ouvrir()
    render(ConfirmModal)
    const vue = render(TirelireImport)
    await fireEvent.click(vue.getByRole('button', { name: 'Supprimer le relevé n° 11 de 2026' }))
    const boite = within(await vue.findByRole('alertdialog'))
    expect(boite.getByRole('heading', { name: 'Supprimer le relevé n° 11 de 2026 ?' })).toBeInTheDocument()
    expect(texte(boite.getByText(/seront supprimées/))).toBe('84 opérations seront supprimées avec lui. Le même PDF pourra être importé de nouveau.')
    await fireEvent.click(boite.getByRole('button', { name: 'Annuler' }))
    await waitFor(() => expect(vue.queryByRole('alertdialog')).toBeNull())
    expect(supprimer).not.toHaveBeenCalled()
    await fireEvent.click(vue.getByRole('button', { name: 'Supprimer le relevé n° 11 de 2026' }))
    await fireEvent.click(within(await vue.findByRole('alertdialog')).getByRole('button', { name: 'Supprimer' }))
    await waitFor(() => expect(vue.queryByRole('button', { name: 'Supprimer le relevé n° 11 de 2026' })).toBeNull())
    expect(supprimer).toHaveBeenCalledWith(11)
    expect(vue.getByRole('button', { name: 'Supprimer le relevé n° 10 de 2026' })).toBeInTheDocument()
  })

  it('suppression refusée : le message s’affiche, le relevé reste', async () => {
    vi.spyOn(tirelireApi, 'supprimer').mockRejectedValue(new ApiError(503, 'aegis_unavailable', ''))
    const { vue } = await monter([releve()])
    await fireEvent.click(vue.getByRole('button', { name: 'Supprimer le relevé n° 11 de 2026' }))
    await fireEvent.click(within(await vue.findByRole('alertdialog')).getByRole('button', { name: 'Supprimer' }))
    expect(await vue.findByText('Tirelire est indisponible pour le moment.')).toBeInTheDocument()
    await waitFor(() => expect(vue.getByRole('button', { name: 'Supprimer le relevé n° 11 de 2026' })).toBeEnabled())
  })
})
