import { notify } from '../../../../web/estompe/src/lib/ui'
import { invoke } from '../../crates/core/ts/tauri'
import { annuler, appliquer, historique, lireProjet, nomTravail, retablir, sourceDe, type EtatExport, type Historique, type Montage, type Ouverture, type Source } from './montage'
import type { Clip } from './replay'
import { chargerClips, replay, texteErreur } from './replay.svelte'

export interface Message { genre: 'ok' | 'att' | 'dgr'; texte: string }

class Editeur {
  preparation = $state<{ chemin: string; nom: string; pct: number | null } | null>(null)
  ouverture = $state.raw<Ouverture | null>(null)
  source = $state.raw<Source | null>(null)
  historique = $state.raw<Historique | null>(null)
  message = $state<Message | null>(null)
}

export const editeur = new Editeur()

let file: Promise<unknown> = Promise.resolve()

function sauver() {
  const o = editeur.ouverture
  const montage = editeur.historique?.present
  if (!o || !montage) return
  file = file.then(() => invoke('montage_enregistrer', { chemin: o.chemin, montage })).catch((e) => { editeur.message = { genre: 'dgr', texte: `Montage non enregistré : ${texteErreur(e)}` } })
}

function remplacer(h: Historique) {
  if (h === editeur.historique) return
  editeur.historique = h
  sauver()
}

export const modifier = (m: Montage) => { if (editeur.historique) remplacer(appliquer(editeur.historique, m)) }
export const defaire = () => { if (editeur.historique) remplacer(annuler(editeur.historique)) }
export const refaire = () => { if (editeur.historique) remplacer(retablir(editeur.historique)) }

export async function ouvrirEditeur(clip: Pick<Clip, 'chemin' | 'nom'>) {
  if (editeur.preparation) return
  editeur.preparation = { chemin: clip.chemin, nom: clip.nom, pct: null }
  try {
    const o = await invoke<Ouverture>('montage_ouvrir', { chemin: clip.chemin })
    const source = sourceDe(o)
    editeur.source = source
    editeur.historique = historique(lireProjet(o.projet, source))
    editeur.message = null
    editeur.ouverture = o
  } catch (e) {
    notify.fail(`Impossible de modifier « ${clip.nom} ». ${texteErreur(e)}`)
    void chargerClips()
  } finally {
    editeur.preparation = null
  }
}

export function fermerEditeur() {
  editeur.ouverture = null
}

export function suivreExport(e: EtatExport) {
  const id = `export:${e.id}`
  const autres = replay.travaux.filter((t) => t.id !== id)
  const ici = editeur.ouverture?.chemin === e.source
  if (e.etat === 'encours') {
    replay.travaux = [...autres, { id, libelle: nomTravail(e), pct: e.pct, source: e.source, annuler: () => void invoke('montage_annuler', { id: e.id }).catch((x) => notify.fail(texteErreur(x))) }]
    return
  }
  replay.travaux = autres
  const message: Message = e.etat === 'fini' ? { genre: e.message ? 'att' : 'ok', texte: `Montage de « ${e.nom} » exporté dans le dossier des clips.${e.message ? ` ${e.message}` : ''}` }
    : e.etat === 'annule' ? { genre: 'att', texte: `Export de « ${e.nom} » annulé : aucun fichier n’a été gardé, le montage est conservé.` }
    : { genre: 'dgr', texte: e.message ?? `Export de « ${e.nom} » impossible.` }
  if (ici) editeur.message = message
  else if (message.genre === 'dgr') notify.fail(message.texte)
  else if (message.genre === 'ok') notify.ok(message.texte)
  else notify.info(message.texte)
  if (e.etat !== 'annule') void chargerClips()
}

export async function exporter() {
  const o = editeur.ouverture
  const montage = editeur.historique?.present
  if (!o || !montage) return
  editeur.message = null
  try {
    await file
    await invoke('montage_exporter', { chemin: o.chemin, montage })
  } catch (e) {
    editeur.message = { genre: 'dgr', texte: `Export impossible. ${texteErreur(e)}` }
  }
}

export async function demarrerMontage() {
  if (!replay.present) return
  const { listen } = await import('@tauri-apps/api/event')
  void listen<EtatExport>('montage://export', ({ payload }) => suivreExport(payload))
  void listen<{ chemin: string; pct: number }>('montage://preparation', ({ payload }) => {
    if (editeur.preparation?.chemin === payload.chemin) editeur.preparation.pct = payload.pct
  })
  try {
    for (const e of await invoke<EtatExport[]>('montage_exports')) suivreExport(e)
  } catch (e) { notify.fail(texteErreur(e)) }
}
