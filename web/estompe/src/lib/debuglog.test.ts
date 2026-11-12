import { describe, expect, it } from 'vitest'
import { mergeEntries, parseLines, type Entry } from './debuglog.svelte'

const ligne = (t: number, s: string, m: string): Entry => ({ t, l: 'info', s, m })

describe('journal debug', () => {
  it('ajoute les lignes natives écrites après l’activation, sans doublon et dans l’ordre', () => {
    const affichees = [ligne(1, 'debug', 'mode debug activé'), ligne(5, 'interface', 'clic')]
    const fichier = parseLines([ligne(1, 'debug', 'mode debug activé'), ligne(3, 'replay', 'tampon armé'), ligne(5, 'interface', 'clic'), ligne(9, 'replay', 'images : 600 émises')].map((e) => JSON.stringify(e)).join('\n'))
    expect(mergeEntries(affichees, fichier).map((e) => e.t)).toEqual([1, 3, 5, 9])
  })

  it('rend la même liste quand le fichier n’apporte rien', () => {
    const affichees = [ligne(1, 'debug', 'mode debug activé')]
    expect(mergeEntries(affichees, [ligne(1, 'debug', 'mode debug activé')])).toBe(affichees)
  })
})
