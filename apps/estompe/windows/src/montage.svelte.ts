import { notify } from '../../../../web/estompe/src/lib/ui'
import { invoke } from '../../crates/core/ts/tauri'
import {
  ajouterClip, annuler, appliquer, clipsDe, CLIPS_MAX, corps, dessinerTexte, formatDe, historique, lireProjet, nomTravail, policeCss, refus, retablir, sourceDe, sourcesDe,
  type ClipOuvert, type EtatExport, type Historique, type Montage, type Ouverture, type Pinceau, type Projet, type Texte,
} from './montage'
import type { Clip } from './replay'
import { chargerClips, replay, texteErreur } from './replay.svelte'

export interface Message { genre: 'ok' | 'att' | 'dgr'; texte: string }

class Editeur {
  preparation = $state<{ nom: string; pct: number | null } | null>(null)
  id = $state<string | null>(null)
  clips = $state.raw<ClipOuvert[]>([])
  historique = $state.raw<Historique | null>(null)
  message = $state<Message | null>(null)
  projets = $state.raw<Projet[]>([])
}

export const editeur = new Editeur()

let ecriture: Promise<void> | null = null
let attente = false

function ecrire(): Promise<void> {
  const [projet, montage] = [editeur.id, editeur.historique?.present]
  attente = false
  if (!projet || !montage) return Promise.resolve()
  return invoke('montage_enregistrer', { projet, montage })
    .catch((e) => { editeur.message = { genre: 'dgr', texte: `Montage non enregistré : ${texteErreur(e)}` } })
    .then(() => (attente ? ecrire() : undefined))
}

function sauver() {
  if (ecriture) { attente = true; return }
  ecriture = ecrire().finally(() => { ecriture = null })
}

function remplacer(h: Historique) {
  if (h === editeur.historique) return
  editeur.historique = h
  sauver()
}

export function modifier(m: Montage): boolean {
  const h = editeur.historique
  if (!h) return false
  const motif = refus(h.present, m)
  if (motif) editeur.message = { genre: 'att', texte: motif }
  else remplacer(appliquer(h, m))
  return !motif
}
export const defaire = () => { if (editeur.historique) remplacer(annuler(editeur.historique)) }
export const refaire = () => { if (editeur.historique) remplacer(retablir(editeur.historique)) }

export async function chargerProjets() {
  try { editeur.projets = await invoke<Projet[]>('montage_projets') } catch (e) { notify.fail(texteErreur(e)) }
}

export async function ouvrirEditeur(cible: { chemin: string; nom: string } | { projet: string; nom: string }) {
  if (editeur.preparation) return
  editeur.preparation = { nom: cible.nom, pct: null }
  try {
    const o = await invoke<Ouverture>('montage_ouvrir', 'chemin' in cible ? { chemin: cible.chemin } : { projet: cible.projet })
    editeur.clips = o.clips
    editeur.historique = historique(lireProjet(o.projet, o.clips.map(sourceDe), cible.nom))
    editeur.message = o.manquants.length
      ? { genre: 'att', texte: `${o.manquants.length > 1 ? 'Clips introuvables' : 'Clip introuvable'} : ${o.manquants.map((c) => `« ${c.replace(/\.mp4$/i, '')} »`).join(', ')}. Le reste du montage est ouvert ; ses passages reviendront si le fichier est remis dans le dossier avant la prochaine modification.` }
      : null
    editeur.id = o.id
  } catch (e) {
    notify.fail(`Impossible d’ouvrir « ${cible.nom} ». ${texteErreur(e)}`)
    void chargerClips()
    void chargerProjets()
  } finally {
    editeur.preparation = null
  }
}

export function fermerEditeur() {
  editeur.id = null
  void chargerProjets()
}

export async function ajouterAuMontage(clip: Pick<Clip, 'chemin' | 'nom'>) {
  const m = editeur.historique?.present
  if (!m || editeur.preparation) return
  if (clipsDe(m).length >= CLIPS_MAX) { editeur.message = { genre: 'att', texte: `Un montage réunit ${CLIPS_MAX} clips au plus : ce clip n’a pas été ajouté.` }; return }
  editeur.preparation = { nom: clip.nom, pct: null }
  try {
    const c = await invoke<ClipOuvert>('montage_clip', { chemin: clip.chemin })
    const present = editeur.historique?.present
    if (!present) return
    if (!editeur.clips.some((x) => x.source === c.source)) editeur.clips = [...editeur.clips, c]
    modifier(ajouterClip(present, sourceDe(c)))
  } catch (e) {
    editeur.message = { genre: 'dgr', texte: `Impossible d’ajouter « ${clip.nom} ». ${texteErreur(e)}` }
    void chargerClips()
  } finally {
    editeur.preparation = null
  }
}

export const policePrete = (t: Texte, largeur: number, hauteur: number) => document.fonts.load(policeCss(t, corps(t, largeur, hauteur)), t.contenu).catch(() => [])

export function tracerTexte(toile: HTMLCanvasElement, t: Texte, largeur: number, hauteur: number) {
  toile.width = largeur
  toile.height = hauteur
  return dessinerTexte(toile.getContext('2d') as unknown as Pinceau, t, largeur, hauteur)
}

async function imageTexte(t: Texte, largeur: number, hauteur: number): Promise<number[]> {
  const toile = document.createElement('canvas')
  await policePrete(t, largeur, hauteur)
  tracerTexte(toile, t, largeur, hauteur)
  const png = await new Promise<Blob | null>((ok) => toile.toBlob(ok, 'image/png'))
  if (!png) throw new Error('image du texte impossible à produire')
  return Array.from(new Uint8Array(await png.arrayBuffer()))
}

export function suivreExport(e: EtatExport) {
  const id = `export:${e.id}`
  const autres = replay.travaux.filter((t) => t.id !== id)
  const ici = editeur.id === e.projet
  if (e.etat === 'encours') {
    replay.travaux = [...autres, { id, libelle: nomTravail(e), pct: e.pct, source: e.projet, annuler: () => void invoke('montage_annuler', { id: e.id }).catch((x) => notify.fail(texteErreur(x))) }]
    return
  }
  replay.travaux = autres
  const message: Message = e.etat === 'fini' ? { genre: e.message ? 'att' : 'ok', texte: `Montage « ${e.nom} » exporté dans le dossier des clips.${e.message ? ` ${e.message}` : ''}` }
    : e.etat === 'annule' ? { genre: 'att', texte: `Export de « ${e.nom} » annulé : aucun fichier n’a été gardé, le montage est conservé.` }
    : { genre: 'dgr', texte: e.message ?? `Export de « ${e.nom} » impossible.` }
  if (ici) editeur.message = message
  else if (message.genre === 'dgr') notify.fail(message.texte)
  else if (message.genre === 'ok') notify.ok(message.texte)
  else notify.info(message.texte)
  if (e.etat !== 'annule') void chargerClips()
}

export async function exporter() {
  const [projet, montage] = [editeur.id, editeur.historique?.present]
  if (!projet || !montage) return
  editeur.message = null
  try {
    const f = formatDe(montage, sourcesDe(editeur.clips))
    const images = await Promise.all(montage.textes.map((t) => imageTexte(t, f.largeur, f.hauteur)))
    await ecriture
    await invoke('montage_exporter', { projet, montage, images })
  } catch (e) {
    editeur.message = { genre: 'dgr', texte: `Export impossible. ${texteErreur(e)}` }
  }
}

export async function demarrerMontage() {
  if (!replay.present) return
  const { listen } = await import('@tauri-apps/api/event')
  void listen<EtatExport>('montage://export', ({ payload }) => suivreExport(payload))
  void listen<{ nom: string; pct: number }>('montage://preparation', ({ payload }) => {
    if (editeur.preparation) editeur.preparation = { nom: payload.nom, pct: payload.pct }
  })
  try {
    for (const e of await invoke<EtatExport[]>('montage_exports')) suivreExport(e)
  } catch (e) { notify.fail(texteErreur(e)) }
}
