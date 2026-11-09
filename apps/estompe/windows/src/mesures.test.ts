import { describe, expect, it } from 'vitest'
import { synthese } from '../scripts/synthese-mesures.mjs'
import { construireExport, echantillonsCsv, FORMAT_MESURES, quantile, SCENARIOS, statsFenetre, statsImages, statsLatences, type Echantillon, type MesureProcessus, type Systeme } from './mesures'

const MO = 1024 * 1024
const p = (pid: number, type: string, role: string, prive: number, cpu: number): MesureProcessus => ({ pid, type, role, prive: prive * MO, ws: prive * 2 * MO, pic_ws: prive * 3 * MO, cpu_ms: cpu })
const echantillons: Echantillon[] = [
  { t: 1000, processus: [p(1, 'hote', 'hote', 50, 0), p(2, 'navigateur', 'commun', 100, 0), p(3, 'rendu', 'principale', 200, 0)] },
  { t: 2000, processus: [p(1, 'hote', 'hote', 50, 100), p(2, 'navigateur', 'commun', 100, 50), p(3, 'rendu', 'principale', 210, 300), p(4, 'rendu', 'overlay-vocal', 40, 0)] },
  { t: 3000, processus: [p(1, 'hote', 'hote', 52, 200), p(2, 'navigateur', 'commun', 100, 100), p(3, 'rendu', 'principale', 220, 600), p(4, 'rendu', 'overlay-vocal', 40, 20)] },
]
const systeme: Systeme = {
  app: '0.88.0', webview2: '131.0.2903.70', windows: 'Windows 11 24H2 (build 26100.2605)', cpu: 'AMD Ryzen 7 5800X', coeurs: 16, ram_mo: 32768,
  gpu: ['NVIDIA GeForce RTX 3070'], facteur_echelle: 1.25, options_webview2: ['--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection'], demarrage_processus: 500,
}
const sondeVide = { images: [], messages: [], saisie: [] }

describe('statistiques des mesures', () => {
  it('calcule les quantiles par interpolation', () => {
    expect(quantile([], 0.5)).toBe(0)
    expect(quantile([3, 1, 2], 0.5)).toBe(2)
    expect(quantile([1, 2, 3, 4], 0.5)).toBe(2.5)
    expect(quantile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.95)).toBeCloseTo(9.55)
  })

  it('compte les images perdues et ignore les pauses de la fenêtre masquée', () => {
    const s = statsImages([16, 17, 16, 30, 50, 5000])
    expect(s.n).toBe(5)
    expect(s.perdues).toBe(2)
    expect(s.moyenne_ms).toBeCloseTo(25.8)
  })

  it('résume les latences', () => {
    expect(statsLatences([10, 30, 20])).toEqual({ n: 3, mediane_ms: 20, p95_ms: 29, max_ms: 30 })
    expect(statsLatences([])).toEqual({ n: 0, mediane_ms: 0, p95_ms: 0, max_ms: 0 })
  })

  it('totalise la mémoire et le CPU, par rôle (overlays à part) et par type de processus', () => {
    const s = statsFenetre(echantillons, sondeVide, 0, 5000)
    expect(s.echantillons).toBe(3)
    expect(s.total.prive).toEqual({ mediane_mo: 400, pic_mo: 412 })
    expect(s.total.ws.pic_mo).toBe(824)
    expect(s.total.cpu_pct).toBe(46)
    expect(s.par_role['overlay-vocal']).toEqual({ prive: { mediane_mo: 40, pic_mo: 40 }, ws: { mediane_mo: 80, pic_mo: 80 }, cpu_pct: 1 })
    expect(s.par_role.principale.cpu_pct).toBe(30)
    expect(Object.keys(s.par_type)).toEqual(['hote', 'navigateur', 'rendu'])
    expect(s.par_type.rendu.prive.pic_mo).toBe(260)
  })

  it('limite les statistiques d’un scénario à sa fenêtre', () => {
    const sonde = { images: [[1500, 16], [2500, 40]] as [number, number][], messages: [[2600, 12]] as [number, number][], saisie: [] }
    const s = statsFenetre(echantillons, sonde, 2000, 3000)
    expect(s.echantillons).toBe(2)
    expect(s.images.n).toBe(1)
    expect(s.images.perdues).toBe(1)
    expect(s.latences.messages.n).toBe(1)
    expect(s.total.cpu_pct).toBe(47)
  })
})

describe('export des mesures', () => {
  const marqueurs = [{ nom: 'repos', debut: 1000, fin: 2000 }]
  const e = construireExport({ debut: 900, fin: 3100, intervalle_ms: 1000, echantillons }, systeme, sondeVide, marqueurs, 2500, Date.UTC(2026, 10, 9))

  it('contient le système, le démarrage, les échantillons et les statistiques par scénario', () => {
    expect(e.format).toBe(FORMAT_MESURES)
    expect(e.app).toBe('0.88.0')
    expect(e.systeme.options_webview2).toHaveLength(1)
    expect(e.demarrage_ms).toBe(2000)
    expect(e.cree).toBe('2026-11-09T00:00:00.000Z')
    expect(Object.keys(e.statistiques.scenarios)).toEqual(['repos'])
    expect(e.statistiques.scenarios.repos.echantillons).toBe(2)
    expect(e.statistiques.session.echantillons).toBe(3)
  })

  it('ne contient aucune donnée personnelle', () => {
    const texte = JSON.stringify(e)
    expect(Object.keys(e).sort()).toEqual(['app', 'cree', 'debut', 'demarrage_ms', 'echantillons', 'fin', 'format', 'intervalle_ms', 'marqueurs', 'sonde', 'statistiques', 'systeme'])
    expect(Object.keys(e.echantillons[0].processus[0]).sort()).toEqual(['cpu_ms', 'pic_ws', 'pid', 'prive', 'role', 'type', 'ws'])
    expect(texte).not.toMatch(/user|pseudo|content|token|mail/i)
  })

  it('ignore un démarrage incohérent', () => {
    expect(construireExport({ debut: 0, fin: 1, intervalle_ms: 1000, echantillons: [] }, systeme, sondeVide, [], null).demarrage_ms).toBeNull()
    expect(construireExport({ debut: 0, fin: 1, intervalle_ms: 1000, echantillons: [] }, systeme, sondeVide, [], 100).demarrage_ms).toBeNull()
  })

  it('exporte les échantillons en CSV, une ligne par processus', () => {
    const csv = echantillonsCsv(echantillons.slice(0, 1)).trim().split('\n')
    expect(csv[0]).toBe('t,pid,type,role,prive_octets,ws_octets,pic_ws_octets,cpu_ms')
    expect(csv).toHaveLength(4)
    expect(csv[1]).toBe(`1000,1,hote,hote,${50 * MO},${100 * MO},${150 * MO},0`)
  })

  it('propose les six scénarios du protocole de l’étude', () => {
    expect(SCENARIOS.map((s) => s.id)).toEqual(['repos', 'gros-salon', 'defilement', 'flux', 'jeu', 'reduite'])
  })
})

describe('synthèse comparative des exports', () => {
  const session = (app: string, options: string[], prive: number, demarrage: number) => {
    const ech = echantillons.map((e) => ({ ...e, processus: e.processus.map((x) => ({ ...x, prive: x.role === 'principale' ? prive * MO : x.prive })) }))
    return construireExport({ debut: 900, fin: 3100, intervalle_ms: 1000, echantillons: ech }, { ...systeme, app, options_webview2: options, demarrage_processus: 0 }, sondeVide, [{ nom: 'repos', debut: 1000, fin: 3000 }], demarrage)
  }

  it('donne la médiane de chaque cible, scénario par scénario', () => {
    const md = synthese([
      ...[200, 300, 250, 220, 900].map((v, i) => session('0.88.0', systeme.options_webview2, v, 1000 + i)),
      ...[180, 190, 185, 170, 175].map((v) => session('0.88.0', [...systeme.options_webview2, '--renderer-process-limit=1'], v, 900)),
    ])
    expect(md).toContain('| Métrique | 0.88.0 · options par défaut (5 sessions) | 0.88.0 · --renderer-process-limit=1 (5 sessions) |')
    expect(md).toContain('| Lancement → première conversation affichée (ms) | 1002 | 900 |')
    expect(md).toContain('## Repos 5 min')
    expect(md).toMatch(/\| Mémoire privée totale, médiane \(Mo\) \| 440 \| 370 \|/)
    expect(md).toContain('| Overlays, mémoire privée médiane (Mo) | 40 | 40 |')
    expect(md).not.toContain('Images : p95')
  })

  it('refuse un fichier qui n’est pas un export de mesures', () => {
    expect(() => synthese([{ format: 'autre' }])).toThrow('format inconnu')
  })
})
