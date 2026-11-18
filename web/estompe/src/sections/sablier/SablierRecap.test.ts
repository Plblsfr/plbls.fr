import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { app } from '../../lib/store.svelte'
import { recap } from './essais'
import SablierRecap from './SablierRecap.svelte'
import { sablierApi } from './sablier'
import { sablier } from './sablier.svelte'

const CLES = ['ouverture', 'usage', 'entracte', 'sillon', 'messaging', 'trophees']

beforeEach(() => {
  sablier.reinitialiser()
  vi.spyOn(sablierApi, 'nonVus').mockResolvedValue({ trophies: 0, recaps: 0 })
  app.details = { s1: { server: { id: 's1', name: 'Amis', owner_id: 'me', icon_version: 0 }, channels: [{ id: 'c1', name: 'général', kind: 'text', position: 0 }], members: [] } }
})
afterEach(() => { vi.restoreAllMocks(); sablier.reinitialiser(); app.details = {} })

async function ouvrir() {
  const lire = vi.spyOn(sablierApi, 'recap').mockResolvedValue(recap())
  const vu = vi.spyOn(sablierApi, 'marquerVus').mockResolvedValue(undefined)
  const onclose = vi.fn()
  const vue = render(SablierRecap, { id: 12, onclose })
  const boite = vue.getByRole('dialog')
  await waitFor(() => expect(boite.querySelector('[data-ecran="ouverture"]')).not.toBeNull())
  const ecran = () => boite.querySelector('[data-ecran]')?.getAttribute('data-ecran')
  return { vue, boite, ecran, onclose, lire, vu }
}

describe('SablierRecap', () => {
  it('C4 : se parcourt en entier au clavier, dans les deux sens, et se quitte avec Échap', async () => {
    const { vue, boite, ecran, onclose, vu } = await ouvrir()
    expect(vu).toHaveBeenCalledWith({ recaps: [12] })
    await waitFor(() => expect(boite).toHaveFocus())
    expect(vue.getByRole('img', { name: 'Écran 1 sur 6' })).toBeInTheDocument()
    for (const cle of CLES.slice(1)) {
      await fireEvent.keyDown(boite, { key: 'ArrowRight' })
      expect(ecran()).toBe(cle)
    }
    await fireEvent.keyDown(boite, { key: 'ArrowRight' })
    expect(ecran()).toBe('trophees')
    expect(vue.getByRole('img', { name: 'Écran 6 sur 6' })).toBeInTheDocument()
    expect(vue.getByRole('button', { name: 'Terminer' })).toBeInTheDocument()
    await fireEvent.keyDown(boite, { key: 'ArrowLeft' })
    expect(ecran()).toBe('messaging')
    expect(vue.queryByRole('button', { name: 'Terminer' })).toBeNull()
    await fireEvent.keyDown(boite, { key: 'Home' })
    expect(ecran()).toBe('ouverture')
    await fireEvent.keyDown(boite, { key: 'ArrowLeft' })
    expect(ecran()).toBe('ouverture')
    expect(onclose).not.toHaveBeenCalled()
    await fireEvent.keyDown(boite, { key: 'Escape' })
    expect(onclose).toHaveBeenCalledTimes(1)
  })

  it('C4 : se parcourt au toucher par les zones gauche et droite et par balayage, et se quitte à tout moment', async () => {
    const { vue, boite, ecran, onclose } = await ouvrir()
    const suivant = vue.getByRole('button', { name: 'Écran suivant' })
    const precedent = vue.getByRole('button', { name: 'Écran précédent' })
    for (const cle of CLES.slice(1)) {
      await fireEvent.click(suivant)
      expect(ecran()).toBe(cle)
    }
    await fireEvent.click(precedent)
    expect(ecran()).toBe('messaging')
    await fireEvent.pointerDown(boite, { clientX: 300, clientY: 400 })
    await fireEvent.pointerUp(boite, { clientX: 120, clientY: 410 })
    expect(ecran()).toBe('trophees')
    await fireEvent.pointerDown(boite, { clientX: 100, clientY: 400 })
    await fireEvent.pointerUp(boite, { clientX: 280, clientY: 390 })
    expect(ecran()).toBe('messaging')
    await fireEvent.pointerDown(boite, { clientX: 100, clientY: 100 })
    await fireEvent.pointerUp(boite, { clientX: 110, clientY: 400 })
    expect(ecran()).toBe('messaging')
    await fireEvent.click(vue.getByRole('button', { name: 'Fermer le récap' }))
    expect(onclose).toHaveBeenCalledTimes(1)
    await fireEvent.click(suivant)
    await fireEvent.click(vue.getByRole('button', { name: 'Revoir' }))
    expect(ecran()).toBe('ouverture')
    await fireEvent.keyDown(boite, { key: 'End' })
    await fireEvent.click(vue.getByRole('button', { name: 'Terminer' }))
    expect(onclose).toHaveBeenCalledTimes(2)
  })

  it('la tabulation reste dans le récap sans y piéger : Échap sort toujours', async () => {
    const { vue, boite, onclose } = await ouvrir()
    const fermer = vue.getByRole('button', { name: 'Fermer le récap' })
    const suivant = vue.getByRole('button', { name: 'Écran suivant' })
    suivant.focus()
    await fireEvent.keyDown(boite, { key: 'Tab' })
    expect(fermer).toHaveFocus()
    await fireEvent.keyDown(boite, { key: 'Tab', shiftKey: true })
    expect(suivant).toHaveFocus()
    await fireEvent.keyDown(suivant, { key: 'Escape' })
    expect(onclose).toHaveBeenCalled()
  })

  it('récap introuvable : message et retour, sans marquer vu', async () => {
    vi.spyOn(sablierApi, 'recap').mockRejectedValue(new ApiError(404, 'not_found', 'introuvable'))
    const vu = vi.spyOn(sablierApi, 'marquerVus').mockResolvedValue(undefined)
    const onclose = vi.fn()
    const vue = render(SablierRecap, { id: 99, onclose })
    expect(await vue.findByRole('alert')).toHaveTextContent('Ce récap n’existe pas ou n’est pas le tien.')
    await fireEvent.click(vue.getByRole('button', { name: 'Retour aux récaps' }))
    expect(onclose).toHaveBeenCalled()
    expect(vu).not.toHaveBeenCalled()
  })
})
