import { voice } from '../../../../web/estompe/src/blocks/vocal/voice.svelte'
import { uploadAttachment } from '../../../../web/estompe/src/blocks/videos/video'
import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'
import { app } from '../../../../web/estompe/src/lib/store.svelte'
import type { ConvKey } from '../../../../web/estompe/src/lib/types'
import { notify } from '../../../../web/estompe/src/lib/ui'
import { invoke } from '../../crates/core/ts/tauri'
import { expirer } from './msg-overlay.svelte'
import { registerGlobalPtt } from './ptt-global'
import {
  ABSENT, ajouterMessage, CLE, DEFAUTS, ecrireReglages, ETAT_INITIAL, libelleMicro, lireReglages, messageClip, messageErreur, messageEtat, reglagesMoteur,
  type Clip, type ClipEnregistre, type Disponibilite, type ErreurReplay, type Etat, type MessageOverlay, type Options, type ReglagesMoteur, type ReglagesReplay, type Stockage,
} from './replay'

export interface Travail { id: string; libelle: string }

export const replay = $state<{
  present: boolean; version: string; reglages: ReglagesReplay; dispo: Disponibilite | null; options: Options | null; erreurOptions: string; etat: Etat
  clips: Clip[]; stockage: Stockage | null; microLibelle: string | null; messages: MessageOverlay[]; travaux: Travail[]
}>({
  present: false, version: '', reglages: { ...DEFAUTS }, dispo: null, options: null, erreurOptions: '', etat: ETAT_INITIAL,
  clips: [], stockage: null, microLibelle: null, messages: [], travaux: [],
})

export const texteErreur = (e: unknown) => (typeof e === 'string' ? e : e instanceof Error ? e.message : String(e))
let convertir = (chemin: string) => chemin
export const urlFichier = (chemin: string) => convertir(chemin)

const moteur = () => reglagesMoteur($state.snapshot(replay.reglages), { libelle: replay.microLibelle, reduction: prefs.noise, gain: prefs.micGain / 100 })

function enregistrer() {
  try { localStorage.setItem(CLE, ecrireReglages($state.snapshot(replay.reglages), replay.version)) } catch { return }
}

export function regler(patch: Partial<ReglagesReplay>) {
  Object.assign(replay.reglages, patch)
  enregistrer()
}

export async function chargerMateriel() {
  try {
    replay.dispo = await invoke<Disponibilite>('replay_disponible')
  } catch (e) {
    replay.dispo = { disponible: false, raison: texteErreur(e), encodeur: null }
  }
  if (!replay.dispo.disponible) { replay.options = null; return }
  try {
    replay.options = await invoke<Options>('replay_options', { reglages: moteur() })
    replay.erreurOptions = ''
  } catch (e) {
    replay.options = null
    replay.erreurOptions = texteErreur(e)
  }
}

export async function chargerClips() {
  try {
    replay.clips = await invoke<Clip[]>('replay_clips')
    replay.stockage = await invoke<Stockage>('replay_stockage')
  } catch (e) { notify.fail(texteErreur(e)) }
}

let file: Promise<unknown> = Promise.resolve()
let envoye = ''
let rangement = ''

function configurer(r: ReglagesMoteur) {
  const cle = JSON.stringify(r)
  if (cle === envoye) return
  envoye = cle
  file = file.then(async () => {
    await invoke('replay_configurer', { reglages: r })
    const lieu = JSON.stringify([r.dossier, r.quotaOctets])
    if (lieu !== rangement) { rangement = lieu; await chargerClips() }
    if (replay.dispo) await chargerMateriel()
  }).catch((e) => notify.fail(texteErreur(e)))
}

const delais = new Map<string, ReturnType<typeof setTimeout>>()
let numero = 0

function annoncer(m: Omit<MessageOverlay, 'id'>) {
  const id = `r${++numero}`
  replay.messages = ajouterMessage(replay.messages, { ...m, id })
  expirer(delais, id, () => { replay.messages = replay.messages.filter((x) => x.id !== id); return true })
}

export const sauvegarderClip = () => invoke('replay_sauvegarder').catch((e) => notify.fail(texteErreur(e)))
export const ouvrirDossier = () => invoke('replay_ouvrir_dossier').catch((e) => notify.fail(texteErreur(e)))

export async function renommerClip(clip: Clip, nom: string) {
  await invoke<string>('replay_renommer', { chemin: clip.chemin, nom })
  await chargerClips()
}

export async function supprimerClips(chemins: string[]) {
  await invoke('replay_supprimer', { chemins })
  await chargerClips()
}

export class TropGros extends Error {}

export async function envoyerClip(clip: Clip, conv: ConvKey, mode: 'direct' | 'h264' | 'leger') {
  const travail = { id: `${clip.chemin}>${conv}`, libelle: `Envoi de « ${clip.nom} »` }
  replay.travaux.push(travail)
  try {
    const chemin = mode === 'direct' ? clip.chemin : await invoke<string>('replay_vers_h264', { chemin: clip.chemin, tailleMaxOctets: mode === 'leger' ? app.maxVideo : null })
    const blob = await (await fetch(urlFichier(chemin))).blob()
    if (blob.size > app.maxVideo) throw new TropGros()
    const piece = await uploadAttachment(new File([blob], `${clip.nom}.mp4`, { type: 'video/mp4' }))
    await app.send(conv, '', [piece.id])
  } finally {
    replay.travaux = replay.travaux.filter((t) => t.id !== travail.id)
  }
}

export async function demarrerReplay() {
  try {
    replay.version = await (await import('@tauri-apps/api/app')).getVersion()
    convertir = (await import('@tauri-apps/api/core')).convertFileSrc
    const etat = await invoke<Etat>('replay_etat')
    if (etat.raison === ABSENT) return
    replay.etat = etat
  } catch { return }
  let brut: string | null = null
  try { brut = localStorage.getItem(CLE) } catch { brut = null }
  replay.reglages = lireReglages(brut, replay.version)
  enregistrer()
  replay.present = true
  const { listen } = await import('@tauri-apps/api/event')
  void listen<Etat>('replay://etat', ({ payload }) => {
    const message = messageEtat(replay.etat, payload)
    const source = replay.etat.jeu !== payload.jeu
    replay.etat = payload
    if (message) annoncer(message)
    if (source && replay.dispo) void chargerMateriel()
  })
  void listen<ClipEnregistre>('replay://clip', ({ payload }) => {
    annoncer(messageClip(payload))
    void chargerClips()
  })
  void listen<ErreurReplay>('replay://erreur', ({ payload }) => {
    annoncer(messageErreur(payload))
    notify.fail(messageErreur(payload).texte)
    if (payload.code === 'quota') void chargerClips()
  })
  $effect.root(() => {
    $effect(() => configurer(moteur()))
    $effect(() => {
      const id = prefs.micId
      if (!replay.reglages.micro) return
      let stop = false
      navigator.mediaDevices.enumerateDevices().then((l) => { if (!stop) replay.microLibelle = libelleMicro(l, id) }).catch(() => { if (!stop) replay.microLibelle = null })
      return () => { stop = true }
    })
    $effect(() => {
      if (!replay.reglages.actif) return
      const touche = replay.reglages.raccourci
      let liberer: (() => Promise<void>) | null = null
      let stop = false
      registerGlobalPtt(touche, (on) => { if (on) void sauvegarderClip() })
        .then((r) => { if (stop) void r(); else liberer = r })
        .catch((e) => notify.info((e as Error).message))
      return () => { stop = true; void liberer?.() }
    })
    $effect(() => {
      const actif = voice.screen
      if (!replay.reglages.actif) return
      void invoke('replay_partage', { actif }).catch(() => {})
    })
  })
  if (replay.reglages.actif) void chargerMateriel()
}
