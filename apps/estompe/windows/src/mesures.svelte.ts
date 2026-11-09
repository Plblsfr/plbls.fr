import { invoke } from '../../crates/core/ts/tauri'
import { sonde } from '../../../../web/estompe/src/lib/sonde'
import { notify } from '../../../../web/estompe/src/lib/ui'
import { construireExport, echantillonsCsv, SCENARIOS, type ExportMesures, type Marqueur, type Releve, type Scenario, type Systeme } from './mesures'

const ATTENTE_DEFILEMENT_MS = 3000
const PAS_DEFILEMENT_PX = 40

export const mesures = $state<{ debut: number | null; scenario: { id: string; fin: number } | null; marqueurs: Marqueur[]; dernier: ExportMesures | null; occupe: boolean }>({
  debut: null, scenario: null, marqueurs: [], dernier: null, occupe: false,
})

let minuterie: ReturnType<typeof setTimeout> | undefined
let finScenario: (() => void) | null = null

export async function demarrerSession() {
  if (mesures.debut !== null || mesures.occupe) return
  mesures.occupe = true
  try {
    mesures.debut = await invoke<number>('mesures_demarrer', {})
    mesures.marqueurs = []
    sonde.demarrer()
  } catch (e) { notify.error(e, 'Impossible de démarrer la mesure') } finally { mesures.occupe = false }
}

function defiler(fin: number) {
  const el = document.querySelector<HTMLElement>('.messages')
  if (!el) return
  let sens = -1
  const pas = () => {
    if (Date.now() >= fin || mesures.scenario === null) return
    const max = el.scrollHeight - el.clientHeight
    if (el.scrollTop <= 0) sens = 1
    else if (el.scrollTop >= max) sens = -1
    el.scrollTop += sens * PAS_DEFILEMENT_PX
    requestAnimationFrame(pas)
  }
  requestAnimationFrame(pas)
}

async function fenetre() {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  return getCurrentWindow()
}

function terminerScenario() {
  clearTimeout(minuterie)
  const fin = finScenario
  finScenario = null
  fin?.()
}

export async function lancerScenario(s: Scenario) {
  if (mesures.debut === null || mesures.scenario) return
  const attente = s.auto === 'defilement' ? ATTENTE_DEFILEMENT_MS : 0
  const debut = Date.now() + attente
  const fin = debut + s.duree
  mesures.scenario = { id: s.id, fin }
  if (s.auto === 'reduire') await (await fenetre()).minimize()
  finScenario = () => {
    sonde.suivreImages(false)
    mesures.marqueurs.push({ nom: s.id, debut, fin: Math.min(Date.now(), fin) })
    mesures.scenario = null
    if (s.auto === 'reduire') void fenetre().then(async (w) => { await w.unminimize(); await w.show() })
    notify.info(`Scénario « ${s.titre} » terminé.`)
  }
  minuterie = setTimeout(() => {
    if (s.images) sonde.suivreImages(true)
    if (s.auto === 'defilement') defiler(fin)
    minuterie = setTimeout(terminerScenario, s.duree)
  }, attente)
}

export const interrompreScenario = terminerScenario

export async function arreterSession() {
  if (mesures.debut === null || mesures.occupe) return
  mesures.occupe = true
  try {
    terminerScenario()
    const releve = await invoke<Releve>('mesures_arreter')
    const donnees = sonde.arreter()
    const systeme = await invoke<Systeme>('mesures_systeme')
    mesures.dernier = construireExport(releve, systeme, donnees, mesures.marqueurs, sonde.premiereConversation)
    mesures.debut = null
  } catch (e) { notify.error(e, 'Impossible d’arrêter la mesure') } finally { mesures.occupe = false }
}

function telecharger(contenu: string, type: string, nom: string) {
  const a = document.createElement('a')
  a.href = URL.createObjectURL(new Blob([contenu], { type }))
  a.download = nom
  a.click()
  URL.revokeObjectURL(a.href)
}

export function exporterMesures() {
  const e = mesures.dernier
  if (!e) return
  const base = `estompe-mesures-${e.app}-${e.cree.slice(0, 19).replace(/[:T]/g, '-')}`
  telecharger(JSON.stringify(e, null, 2), 'application/json', `${base}.json`)
  telecharger(echantillonsCsv(e.echantillons), 'text/csv', `${base}.csv`)
}

export const scenario = (id: string) => SCENARIOS.find((s) => s.id === id)
