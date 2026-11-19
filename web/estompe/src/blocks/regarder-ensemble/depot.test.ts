import { describe, expect, it } from 'vitest'
import { dossierDe, memoire, ouvrirDepot, purgerRestes, RACINE, RESTES_MS, type Dossier } from './depot'

interface Feuille { kind: 'file'; data: Blob; modifie: number }
interface Faux extends Dossier { kind: 'directory'; enfants: Map<string, Faux | Feuille>; refuse: boolean }

function dossier(horloge: () => number = Date.now): Faux {
  const enfants: Faux['enfants'] = new Map()
  const d: Faux = {
    kind: 'directory', enfants, refuse: false,
    async getDirectoryHandle(name, o) {
      let e = enfants.get(name)
      if (!e && o?.create) { e = dossier(horloge); enfants.set(name, e) }
      if (e?.kind !== 'directory') throw new DOMException('absent', 'NotFoundError')
      return e
    },
    async getFileHandle(name, o) {
      if (!enfants.has(name) && !o?.create) throw new DOMException('absent', 'NotFoundError')
      return {
        kind: 'file',
        async getFile() {
          const e = enfants.get(name)
          if (e?.kind !== 'file') throw new DOMException('absent', 'NotFoundError')
          return Object.assign(e.data, { lastModified: e.modifie })
        },
        async createWritable() {
          if (d.refuse) throw new DOMException('plein', 'QuotaExceededError')
          let data = new Blob([])
          return { async write(b: Blob) { data = b }, async close() { enfants.set(name, { kind: 'file', data, modifie: horloge() }) } }
        },
      }
    },
    async removeEntry(name) { if (!enfants.delete(name)) throw new DOMException('absent', 'NotFoundError') },
    async *entries() { for (const [n, e] of enfants) yield [n, { kind: e.kind }] as [string, { kind: string }] },
  }
  return d
}

const blob = (n: number) => new Blob([new Uint8Array(n)])
const sous = (racine: Faux, ...chemin: string[]) => chemin.reduce((d, n) => d.enfants.get(n) as Faux, racine)

describe('segments gardés sur le disque de l’appareil', () => {
  it('garde en mémoire quand le flux n’a pas de clé ou que l’appareil n’a pas de stockage', async () => {
    const racine = dossier()
    for (const d of [await ouvrirDepot(null, racine), await ouvrirDepot('f-480p-0-g', null)]) {
      await d.put('seg0.ts', blob(5))
      expect([d.has('seg0.ts'), d.bytes()]).toEqual([true, 5])
    }
    expect(racine.enfants.size).toBe(0)
  })

  it('écrit les segments sur le disque et les retrouve après un rechargement de la page', async () => {
    const racine = dossier()
    const a = await ouvrirDepot('f-480p-0-g', racine)
    await a.put('seg00000.ts', blob(10))
    await a.put('seg00001.ts', blob(20))
    await a.put('seg00001.ts', blob(99))
    expect([a.bytes(), a.names()]).toEqual([30, ['seg00000.ts', 'seg00001.ts']])
    expect([...sous(racine, RACINE, 'f-480p-0-g').enfants.keys()].sort()).toEqual(['.vu', 'seg00000.ts', 'seg00001.ts'])
    await a.close(false)

    const b = await ouvrirDepot('f-480p-0-g', racine)
    expect([b.bytes(), b.has('seg00001.ts'), b.has('.vu')]).toEqual([30, true, false])
    expect((await b.get('seg00001.ts'))!.size).toBe(20)
    expect(await b.get('seg00009.ts')).toBeNull()
    b.drop('seg00000.ts')
    expect([b.bytes(), b.has('seg00000.ts')]).toEqual([20, false])
    await Promise.resolve()
    expect(sous(racine, RACINE, 'f-480p-0-g').enfants.has('seg00000.ts')).toBe(false)
  })

  it('sépare les qualités et les pistes, et supprime les segments d’un titre à sa fermeture', async () => {
    const racine = dossier()
    const a = await ouvrirDepot('f-480p-0-g', racine)
    await a.put('seg00000.ts', blob(10))
    const b = await ouvrirDepot('f-720p-0-g', racine)
    expect(b.has('seg00000.ts')).toBe(false)
    await b.put('seg00000.ts', blob(40))
    await a.close(true)
    expect([...sous(racine, RACINE).enfants.keys()]).toEqual(['f-720p-0-g'])
    expect(a.bytes()).toBe(0)
    await a.put('seg00001.ts', blob(10))
    expect(a.has('seg00001.ts')).toBe(false)
    expect(dossierDe('../a b/é')).toBe('___a_b__')
  })

  it('continue en mémoire quand l’appareil refuse l’écriture', async () => {
    const racine = dossier()
    const a = await ouvrirDepot('f-480p-0-g', racine)
    await a.put('seg00000.ts', blob(10))
    sous(racine, RACINE, 'f-480p-0-g').refuse = true
    await a.put('seg00001.ts', blob(20))
    expect([a.has('seg00001.ts'), a.bytes(), a.names()]).toEqual([true, 30, ['seg00000.ts', 'seg00001.ts']])
    expect((await a.get('seg00001.ts'))!.size).toBe(20)
    a.drop('seg00001.ts')
    expect(a.bytes()).toBe(10)

    const plein = dossier()
    plein.refuse = true
    const b = await ouvrirDepot('f-480p-0-g', { ...plein, getDirectoryHandle: async () => { throw new DOMException('refusé', 'SecurityError') } })
    await b.put('seg00000.ts', blob(3))
    expect(b.bytes()).toBe(3)
  })

  it('purge les restes d’une séance interrompue, pas ceux d’une séance récente', async () => {
    let maintenant = 1_000_000
    const racine = dossier(() => maintenant)
    await (await ouvrirDepot('vieux', racine)).put('seg00000.ts', blob(10))
    maintenant += RESTES_MS + 1
    await (await ouvrirDepot('recent', racine)).put('seg00000.ts', blob(10))
    sous(racine, RACINE).enfants.set('sans-marque', dossier())
    await purgerRestes(racine, null, maintenant + 1000)
    expect([...sous(racine, RACINE).enfants.keys()]).toEqual(['recent'])
    await purgerRestes(racine, 'recent', maintenant + 2 * RESTES_MS)
    expect([...sous(racine, RACINE).enfants.keys()]).toEqual(['recent'])
    await purgerRestes(null)
    await purgerRestes(dossier())
  })

  it('compte les octets du dépôt en mémoire', async () => {
    const m = memoire()
    await m.put('a', blob(4))
    await m.put('a', blob(6))
    await m.put('b', blob(1))
    expect([m.bytes(), m.names()]).toEqual([7, ['a', 'b']])
    m.drop('a')
    expect(m.bytes()).toBe(1)
    await m.close(true)
    expect([m.bytes(), m.has('b')]).toEqual([0, false])
  })
})
