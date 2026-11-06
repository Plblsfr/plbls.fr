import { describe, it, expect } from 'vitest'
import { DRIVE_DEFAULTS, parseChoices, parseDrive } from './drive'

describe('parseDrive', () => {
  it('lit les réglages du pont', () => {
    const s = parseDrive('{"engine":"org.rhvoice","rate":1.5,"readContent":false,"confirm":false,"autoBluetooth":true,"bluetooth":"AA:BB"}')
    expect(s).toEqual({ engine: 'org.rhvoice', rate: 1.5, readContent: false, confirm: false, autoBluetooth: true, bluetooth: 'AA:BB' })
  })
  it('retombe sur les défauts si le JSON est invalide ou mal typé', () => {
    expect(parseDrive('pas du json')).toEqual(DRIVE_DEFAULTS)
    expect(parseDrive('{"rate":"vite","confirm":"oui","engine":3}')).toEqual(DRIVE_DEFAULTS)
    expect(parseDrive('null')).toEqual(DRIVE_DEFAULTS)
  })
  it('borne la vitesse de la voix', () => {
    expect(parseDrive('{"rate":9}').rate).toBe(2)
    expect(parseDrive('{"rate":0}').rate).toBe(0.5)
  })
})

describe('parseChoices', () => {
  it('ne garde que les entrées complètes', () => {
    expect(parseChoices('[{"id":"a","label":"A"},{"id":"b"},3]')).toEqual([{ id: 'a', label: 'A' }])
    expect(parseChoices('{}')).toEqual([])
    expect(parseChoices('x')).toEqual([])
  })
})
