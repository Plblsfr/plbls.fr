import { describe, it, expect } from 'vitest'
import { CIBLE, manifeste } from '../../../../scripts/estompe/natif-manifeste.mjs'
import { policy } from '../../../../scripts/estompe/update-policy.mjs'

const JSON_MANIFESTE = `{
  "version": "0.2.0",
  "notes": "Notifications du système.",
  "pub_date": "2026-11-20T12:00:00.000Z",
  "mandatory": false,
  "force": false,
  "minVersion": "",
  "message": "",
  "platforms": {
    "windows-x86_64": {
      "url": "https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-v0.2.0/Estompe-natif-beta-0.2.0-setup.exe",
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QK",
      "taille": 31457280
    }
  }
}`

describe('manifeste de l\'app native', () => {
  it('format de latest.json de Tauri, plus la taille', () => {
    const m = manifeste({
      version: '0.2.0',
      notes: 'Notifications du système.',
      politique: policy('natif', { platforms: { natif: {} } }),
      url: 'https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-v0.2.0/Estompe-natif-beta-0.2.0-setup.exe',
      signature: 'dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QK',
      taille: 31457280,
      date: '2026-11-20T12:00:00.000Z',
    })
    expect(JSON.stringify(m, null, 2)).toBe(JSON_MANIFESTE)
    expect(CIBLE).toBe('windows-x86_64')
  })
  it('reprend la politique de platforms.natif', () => {
    const politique = policy('natif', { mandatory: false, platforms: { natif: { mandatory: true, minVersion: '0.2.0', message: 'Mettez à jour' } } })
    expect(manifeste({ version: '0.3.0', notes: '', politique, url: 'u', signature: 's', taille: 1, date: 'd' })).toMatchObject({ mandatory: true, force: true, minVersion: '0.2.0', message: 'Mettez à jour' })
  })
})
