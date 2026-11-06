import { describe, expect, it, beforeEach } from 'vitest'
import { confirmation, confirmer, peutValider, saisir } from './confirm.svelte'
import { ctxmenu, flipY } from './ctxmenu.svelte'

describe('confirmer', () => {
  beforeEach(() => (confirmation.en = null))

  it('renvoie vrai quand on valide', async () => {
    const p = confirmer({ titre: 'Supprimer' })
    confirmation.en!.resoudre({ ok: true, valeur: '' })
    expect(await p).toBe(true)
    expect(confirmation.en).toBeNull()
  })

  it('renvoie faux quand on annule', async () => {
    const p = confirmer({ titre: 'Supprimer', destructif: true })
    confirmation.en!.resoudre({ ok: false, valeur: '' })
    expect(await p).toBe(false)
  })

  it('annule la demande précédente quand une autre arrive', async () => {
    const a = confirmer({ titre: 'A' })
    const b = confirmer({ titre: 'B' })
    expect(await a).toBe(false)
    expect(confirmation.en?.titre).toBe('B')
    confirmation.en!.resoudre({ ok: true, valeur: '' })
    expect(await b).toBe(true)
  })

  it('saisir renvoie le texte, ou null si annulé', async () => {
    const d = { titre: 'Renommer', saisie: { label: 'Nom', valeur: 'x' } }
    const p = saisir(d)
    confirmation.en!.resoudre({ ok: true, valeur: 'y' })
    expect(await p).toBe('y')
    const q = saisir(d)
    confirmation.en!.resoudre({ ok: false, valeur: 'y' })
    expect(await q).toBeNull()
  })
})

describe('peutValider', () => {
  const double = { titre: 'Supprimer', saisie: { label: 'Nom', exiger: 'Mon serveur' } }
  it('exige le nom exact pour la double confirmation', () => {
    expect(peutValider(double, '', false)).toBe(false)
    expect(peutValider(double, 'mon serveur', false)).toBe(false)
    expect(peutValider(double, ' Mon serveur ', false)).toBe(true)
  })
  it('exige la case quand elle est demandée', () => {
    expect(peutValider({ titre: 'x', case: 'Je comprends' }, '', false)).toBe(false)
    expect(peutValider({ titre: 'x', case: 'Je comprends' }, '', true)).toBe(true)
  })
  it('valide d\'office sans condition', () => {
    expect(peutValider({ titre: 'x' }, '', false)).toBe(true)
  })
})

describe('flipY', () => {
  it('reste sous le bouton quand il y a de la place', () => {
    expect(flipY(100, 150, 600, 80)).toBe(100)
  })
  it('se retourne au-dessus du bouton en bas de la fenêtre', () => {
    expect(flipY(560, 150, 600, 540)).toBe(390)
  })
  it('reste en place sans ancre ou si le haut manque aussi', () => {
    expect(flipY(560, 150, 600, undefined)).toBe(560)
    expect(flipY(300, 400, 500, 100)).toBe(300)
  })
})

describe('menu ouvert depuis un bouton', () => {
  it('mémorise le bouton pour lui rendre le focus', async () => {
    const { menuFrom } = await import('./ctxmenu.svelte')
    const b = document.createElement('button')
    menuFrom(b, [])
    expect(ctxmenu.at?.opener).toBe(b)
    ctxmenu.at = null
  })
})

const fichiers = import.meta.glob(['../**/*.ts', '../**/*.svelte', '!../vendor/**', '!../**/*.test.ts'], { query: '?raw', import: 'default', eager: true }) as Record<string, string>

describe('boîtes natives interdites', () => {
  it('aucun confirm/alert/prompt dans src/', () => {
    const re = /(^|[^.\w$])(window\.)?(confirm|alert|prompt)\s*\(/
    const trouves = Object.entries(fichiers).filter(([, t]) => t.split('\n').some((l) => re.test(l))).map(([f]) => f)
    expect(Object.keys(fichiers).length).toBeGreaterThan(100)
    expect(trouves).toEqual([])
  })
})
