<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import Icon from '../../../../web/estompe/src/components/Icon.svelte'
  import Modal from '../../../../web/estompe/src/components/Modal.svelte'
  import {
    actionClavier, ajouterTexte, axe, bloc, bornerCadre, bornerTexte, bornes, cadencesPossibles, cadreAu, CADRE_PLEIN, clipsDe, couper, deplacer, deplacerClip, duree, dureeFinale, formatDe, formaterTemps, gain, HABILLAGES,
    hauteursPossibles, imageDe, lieu, lireTemps, pas, pics, POLICES, position, RACCOURCIS, RAPPORTS, recadrer, regler, reglerSegment, reglerSortie, reglerTexte, retirerClip, retirerTexte, rogner, saccade, sourceDe, sourcesDe,
    suite, tempsImage, TEXTE_MAX, textesA, traceOnde, versAxe, versMontage, VITESSES, type Action, type Cadre, type ClipOuvert, type Cote, type Habillage, type Lieu, type Montage, type Rapport, type Texte,
  } from './montage'
  import { ajouterAuMontage, defaire, editeur, exporter, fermerEditeur, modifier, policePrete, refaire, tracerTexte } from './montage.svelte'
  import { dureeClip } from './replay'
  import { replay, urlFichier } from './replay.svelte'

  let { onmenu }: { onmenu: () => void } = $props()

  const POINTS = 600
  const parImage = 'requestVideoFrameCallback' in HTMLVideoElement.prototype
  const sansExtension = (nom: string) => nom.replace(/\.mp4$/i, '')
  const pourcent = (x: number) => `${x * 100}%`

  const m = $derived(editeur.historique!.present)
  const clips = $derived(editeur.clips)
  const sources = $derived(sourcesDe(clips))

  let racine = $state<HTMLElement>()
  let ecran = $state<HTMLElement>()
  let ligne = $state<HTMLElement>()
  let videos = $state<Record<string, HTMLVideoElement>>({})
  let ou = $state<Lieu>({ i: 0, t: untrack(() => m.segments[0].debut) })
  let lecture = $state(false)
  let illisibles = $state<Record<string, boolean>>({})
  let choisi = $state(-1)
  let glisse = $state<{ cote: Cote; t: number; i: number; total: number } | null>(null)
  let ecoute = $state<Record<number, number>>({})
  let son = $state<Record<string, 'chargement' | 'pret' | 'erreur'>>({})
  let ondes = $state<Record<string, string[]>>({})
  let recadrage = $state(false)
  let cadreGlisse = $state<Cadre | null>(null)
  let texteChoisi = $state(-1)
  let texteGlisse = $state<{ k: number; patch: Partial<Texte> } | null>(null)
  let boites = $state<Record<number, { x: number; y: number; l: number; h: number }>>({})
  let ajout = $state(false)
  let sortie = $state(false)

  const i = $derived(Math.min(ou.i, m.segments.length - 1))
  const seg = $derived(m.segments[i])
  const t = $derived(Math.min(seg.fin, Math.max(seg.debut, ou.t)))
  const u = $derived(position(m, { i, t }))
  const cible = $derived(choisi >= 0 && choisi < m.segments.length ? choisi : i)
  const segCible = $derived(m.segments[cible])
  const srcCible = $derived(sources[segCible.source])
  const clipCible = $derived(clips.find((c) => c.source === segCible.source)!)
  const debutBloc = $derived(bloc(m, cible)[0])
  const vue = $derived(glisse ? rogner(m, sources[m.segments[glisse.i].source], glisse.cote, glisse.t, glisse.i) : m)
  const ligneTemps = $derived(axe(vue))
  const total = $derived(glisse?.total ?? ligneTemps.total)
  const b = $derived(bornes(vue, debutBloc))
  const finale = $derived(dureeFinale(vue))
  const format = $derived(formatDe(m, sources))
  const temps = $derived(versMontage(m, u))
  const ordre = $derived(clipsDe(m))
  const nombrePistes = $derived(Math.max(0, ...ordre.map((c) => sources[c].pistes)))
  const cadre = $derived(cadreGlisse ?? m.cadre)
  const textes = $derived(m.textes.map((x, k) => (texteGlisse?.k === k ? bornerTexte({ ...x, ...texteGlisse.patch }, dureeFinale(m)) : x)))
  const visibles = $derived(recadrage ? [] : textesA({ ...m, textes }, temps))
  const texte = $derived(textes[texteChoisi])
  const travail = $derived(replay.travaux.find((x) => x.source === editeur.id && x.pct !== undefined))
  const disponibles = $derived(replay.clips.filter((c) => !ordre.includes(c.chemin.split(/[\\/]/).pop()!)))
  const pc = (x: number) => pourcent(x / total)

  let ctx: AudioContext | null = null
  const tampons: Record<string, AudioBuffer[]> = {}
  const gains: Record<string, GainNode[]> = {}
  const lecteurs: Record<string, HTMLAudioElement[]> = {}
  let joues: (AudioBufferSourceNode | HTMLAudioElement)[] = []
  let depart = { horloge: 0, t: 0 }
  let detruit = false

  const actif = () => videos[seg.source]
  function arreterSon() {
    for (const s of joues) {
      if (s instanceof HTMLAudioElement) s.pause()
      else { s.onended = null; s.stop(); s.disconnect() }
    }
    joues = []
  }
  function lecteursDe(c: ClipOuvert, contexte: AudioContext) {
    return (lecteurs[c.source] ??= c.pistes.map((p, k) => {
      const a = new Audio()
      a.crossOrigin = 'anonymous'
      a.preservesPitch = true
      a.src = urlFichier(p.chemin)
      contexte.createMediaElementSource(a).connect(gains[c.source][k])
      return a
    }))
  }
  function demarrerSon(instant: number) {
    arreterSon()
    const [c, clip] = [ctx, clips.find((x) => x.source === seg.source)]
    if (!c || !clip || son[clip.source] !== 'pret' || !clip.pistes.length) return
    void c.resume()
    if (seg.vitesse === 1) {
      joues = tampons[clip.source].flatMap((tampon, k) => {
        if (instant >= tampon.duration) return []
        const s = c.createBufferSource()
        s.buffer = tampon
        s.connect(gains[clip.source][k])
        s.start(0, Math.max(0, instant))
        return [s]
      })
    } else {
      joues = lecteursDe(clip, c).map((a) => {
        a.playbackRate = seg.vitesse
        a.currentTime = Math.max(0, instant)
        a.play().catch(() => {})
        return a
      })
    }
    depart = { horloge: c.currentTime, t: instant }
  }
  function recalerSon(instant: number) {
    const premier = joues[0]
    if (!ctx || !premier) return
    const estime = premier instanceof HTMLAudioElement ? premier.currentTime : depart.t + ctx.currentTime - depart.horloge
    if (Math.abs(estime - instant) > (seg.vitesse === 1 ? 0.04 : 0.1)) demarrerSon(instant)
  }
  async function chargerSon(c: ClipOuvert) {
    son[c.source] = c.pistes.length ? 'chargement' : 'pret'
    if (!c.pistes.length) return
    try {
      const contexte = (ctx ??= new AudioContext())
      gains[c.source] = c.pistes.map(() => { const g = contexte.createGain(); g.connect(contexte.destination); return g })
      const lus = await Promise.all(c.pistes.map(async (p) => contexte.decodeAudioData(await (await fetch(urlFichier(p.chemin))).arrayBuffer())))
      if (detruit) return
      const entier = duree(sourceDe(c))
      tampons[c.source] = lus
      ondes[c.source] = lus.map((x) => traceOnde(pics(Array.from({ length: x.numberOfChannels }, (_, k) => x.getChannelData(k)), Math.max(1, Math.round((POINTS * Math.min(x.duration, entier)) / entier)))))
      son[c.source] = 'pret'
      const v = actif()
      if (v && !v.paused && seg.source === c.source) demarrerSon(v.currentTime)
    } catch {
      son[c.source] = 'erreur'
    }
  }
  $effect(() => {
    const nouveaux = clips.filter((c) => !(c.source in untrack(() => son)))
    untrack(() => { for (const c of nouveaux) void chargerSon(c) })
  })
  $effect(() => {
    for (const c of clips) {
      if (son[c.source] !== 'pret') continue
      gains[c.source]?.forEach((g, k) => { g.gain.value = c.source === seg.source && seg.muet ? 0 : c.source === segCible.source && ecoute[k] !== undefined ? ecoute[k] : gain(m, sources[c.source], k) })
    }
  })
  onDestroy(() => {
    detruit = true
    arreterSon()
    void ctx?.close()
  })

  function placer(l: Lieu, jouer = false, avant = actif()) {
    const s = m.segments[l.i]
    const ips = sources[s.source].ips
    const image = Math.min(Math.round(s.fin * ips) - 1, Math.max(Math.round(s.debut * ips), imageDe(l.t, ips)))
    ou = { i: l.i, t: image / ips }
    const v = videos[s.source]
    if (avant && avant !== v) avant.pause()
    if (!v) return
    v.playbackRate = s.vitesse
    v.currentTime = tempsImage(image, ips)
    if (jouer && v.paused) v.play().catch(() => (lecture = false))
  }
  function aller(vers: number) {
    actif()?.pause()
    placer(lieu(m, Math.min(Math.max(0, vers), axe(m).total - 1e-6)))
  }
  function suivre(instant: number) {
    const ips = sources[seg.source].ips
    ou = { i, t: Math.min(Math.round(seg.fin * ips) - 1, Math.max(Math.round(seg.debut * ips), imageDe(instant, ips))) / ips }
    const s = suite(m, { i, t: instant + 1 / ips }, sources)
    if (!s) return recalerSon(instant)
    if ('fin' in s) actif()?.pause()
    else placer(s.aller, true)
  }
  function image(v: HTMLVideoElement, instant: number) {
    if (v.paused || v !== actif()) return
    if (!v.seeking) suivre(instant)
    if (!v.paused && v === actif()) v.requestVideoFrameCallback((_, meta) => image(v, meta.mediaTime))
  }
  const evenement = (e: Event, faire: (v: HTMLVideoElement) => void) => { if (e.currentTarget === actif()) faire(e.currentTarget as HTMLVideoElement) }
  function demarre(v: HTMLVideoElement) {
    lecture = true
    if (parImage) v.requestVideoFrameCallback((_, meta) => image(v, meta.mediaTime))
  }
  function basculerLecture() {
    const v = actif()
    if (!v || illisibles[seg.source]) return
    if (!v.paused) return v.pause()
    const premier = m.segments.findIndex((s) => s.garde)
    if (premier < 0) return
    const s = suite(m, { i, t: t + 1 / sources[seg.source].ips }, sources)
    if (s) placer('fin' in s ? { i: premier, t: m.segments[premier].debut } : s.aller, true)
    else v.play().catch(() => (lecture = false))
  }
  function image1(n: number) {
    actif()?.pause()
    placer(pas(m, { i, t }, n, sources))
  }
  function changer(apres: Montage, selection = -1) {
    const [avant, v] = [u, actif()]
    if (!modifier(apres)) return
    choisi = selection
    v?.pause()
    placer(lieu(m, Math.min(avant, axe(m).total - 1e-6)), false, v)
  }
  function historique(faire: () => void) {
    const [avant, v] = [u, actif()]
    faire()
    choisi = -1
    texteChoisi = -1
    v?.pause()
    placer(lieu(m, Math.min(avant, axe(m).total - 1e-6)), false, v)
  }
  const basculerSegment = () => modifier(reglerSegment(m, cible, { garde: !segCible.garde }))
  const couperIci = () => changer(couper(m, sources[seg.source], t, i))
  const ACTIONS: Record<Action, () => void> = {
    lecture: basculerLecture, precedente: () => image1(-1), suivante: () => image1(1), couper: couperIci,
    annuler: () => historique(defaire), retablir: () => historique(refaire), basculer: basculerSegment,
  }
  function clavier(e: KeyboardEvent) {
    const cibleClavier = e.target as HTMLElement
    if (!racine || !(racine.contains(cibleClavier) || cibleClavier === document.body) || e.defaultPrevented || ajout || sortie) return
    const action = actionClavier(e, cibleClavier.matches('input:not([type="range"]), textarea, select, [contenteditable="true"]'))
    if (!action || (action === 'lecture' && cibleClavier.matches('button, a')) || (['lecture', 'precedente', 'suivante'].includes(action) && cibleClavier.matches('input[type="range"]'))) return
    e.preventDefault()
    ACTIONS[action]()
  }

  const axeA = (e: PointerEvent) => {
    const r = ligne!.getBoundingClientRect()
    return Math.min(total, Math.max(0, ((e.clientX - r.left) / Math.max(1, r.width)) * total))
  }
  const capture = (e: PointerEvent) => {
    const el = e.currentTarget as HTMLElement
    if (e.type === 'pointerdown') el.setPointerCapture(e.pointerId)
    return el.hasPointerCapture(e.pointerId)
  }
  function tete(e: PointerEvent) {
    if (capture(e) && ligne) aller(axeA(e))
  }
  let prise = { x: 0, y: 0, borne: 0, debut: 0, fin: 0, cadre: CADRE_PLEIN as Cadre }
  function poignee(e: PointerEvent, cote: Cote) {
    if (e.type === 'pointerdown') prise = { ...prise, x: e.clientX, borne: bornes(m, cible)[cote] }
    if (!capture(e) || !ligne) return
    const instant = prise.borne + ((e.clientX - prise.x) / Math.max(1, ligne.getBoundingClientRect().width)) * (glisse?.total ?? ligneTemps.total)
    if (e.type === 'pointerup') {
      const fin = glisse
      glisse = null
      if (fin) changer(rogner(m, sources[m.segments[fin.i].source], fin.cote, fin.t, fin.i))
    } else glisse = { cote, t: instant, i: cible, total: glisse?.total ?? ligneTemps.total }
  }
  function poigneeClavier(e: KeyboardEvent, cote: Cote) {
    const actuel = bornes(m, cible)[cote]
    const saut = e.shiftKey ? 1 : 1 / srcCible.ips
    const vers = e.key === 'ArrowLeft' ? actuel - saut : e.key === 'ArrowRight' ? actuel + saut : e.key === 'Home' ? 0 : e.key === 'End' ? srcCible.dureeS : null
    if (vers === null) return
    e.preventDefault()
    e.stopPropagation()
    changer(rogner(m, srcCible, cote, vers, cible), cote === 'debut' ? debutBloc : -1)
  }
  function teteClavier(e: KeyboardEvent) {
    const saut = e.shiftKey ? sources[seg.source].ips : 1
    const vers = e.key === 'ArrowLeft' ? pas(m, { i, t }, -saut, sources) : e.key === 'ArrowRight' ? pas(m, { i, t }, saut, sources) : e.key === 'Home' ? lieu(m, 0) : e.key === 'End' ? lieu(m, axe(m).total) : null
    if (vers === null) return
    e.preventDefault()
    e.stopPropagation()
    actif()?.pause()
    placer(vers)
  }
  function saisie(e: Event, appliquerTemps: (instant: number) => void, actuel: () => number) {
    const champ = e.currentTarget as HTMLInputElement
    const lu = lireTemps(champ.value)
    champ.setAttribute('aria-invalid', String(lu === null))
    if (lu === null) return
    appliquerTemps(lu)
    champ.value = formaterTemps(actuel())
  }
  const volume = (k: number) => m.volumes[segCible.source]?.[k] ?? { volume: 1, muet: false }
  function reglerVolume(k: number, valeur: number) {
    delete ecoute[k]
    modifier(regler(m, srcCible, k, { volume: valeur / 100 }))
  }
  function choisir(j: number) {
    choisi = j
    ecoute = {}
  }

  let porte = { genre: '', de: -1 }
  const saisir = (e: DragEvent, genre: string, de: number) => { porte = { genre, de }; e.dataTransfer?.setData('text/plain', String(de)) }
  const survoler = (e: DragEvent, genre: string) => { if (porte.genre === genre) e.preventDefault() }
  function deposer(e: DragEvent, genre: string, vers: number) {
    if (porte.genre !== genre) return
    e.preventDefault()
    changer(genre === 'segment' ? deplacer(m, porte.de, vers) : deplacerClip(m, ordre[porte.de], vers), genre === 'segment' ? vers : -1)
    porte = { genre: '', de: -1 }
  }

  function tirerCadre(e: PointerEvent, taille: boolean) {
    e.stopPropagation()
    if (e.type === 'pointerdown') prise = { ...prise, x: e.clientX, y: e.clientY, cadre: m.cadre }
    if (!capture(e) || !ecran) return
    const r = ecran.getBoundingClientRect()
    const [dx, dy, c] = [(e.clientX - prise.x) / Math.max(1, r.width), (e.clientY - prise.y) / Math.max(1, r.height), prise.cadre]
    const apres = bornerCadre(taille ? { ...c, l: Math.min(1 - c.x, c.l + dx), h: Math.min(1 - c.y, c.h + dy) } : { ...c, x: c.x + dx, y: c.y + dy }, format.toile)
    if (e.type === 'pointerup') { cadreGlisse = null; modifier(recadrer(m, apres, format.toile)) } else cadreGlisse = apres
  }
  function cadreClavier(e: KeyboardEvent) {
    const d = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key]
    if (!d) return
    e.preventDefault()
    e.stopPropagation()
    const [dx, dy, c] = [d[0] / 100, d[1] / 100, m.cadre]
    modifier(recadrer(m, e.shiftKey ? { ...c, l: Math.min(1 - c.x, c.l + dx), h: Math.min(1 - c.y, c.h + dy) } : { ...c, x: c.x + dx, y: c.y + dy }, format.toile))
  }

  function peindre(toile: HTMLCanvasElement, p: { t: Texte; k: number; l: number; h: number }) {
    let tour = 0
    const dessiner = (q: typeof p) => {
      const mien = ++tour
      void policePrete(q.t, q.l, q.h).then(() => { if (mien === tour) boites[q.k] = tracerTexte(toile, q.t, q.l, q.h) })
    }
    dessiner(p)
    return { update: dessiner, destroy: () => { tour = -1 } }
  }
  function choisirTexte(k: number) {
    texteChoisi = k
    const x = m.textes[k]
    if (x && (temps < x.debut || temps >= x.fin)) aller(versAxe(m, x.debut))
  }
  function tirerTexte(e: PointerEvent, k: number) {
    if (e.type === 'pointerdown') { prise = { ...prise, x: e.clientX, y: e.clientY, debut: m.textes[k].x, fin: m.textes[k].y }; texteChoisi = k }
    if (!capture(e) || !ecran) return
    const r = ecran.getBoundingClientRect()
    const patch = { x: prise.debut + (e.clientX - prise.x) / Math.max(1, r.width), y: prise.fin + (e.clientY - prise.y) / Math.max(1, r.height) }
    if (e.type === 'pointerup') { texteGlisse = null; modifier(reglerTexte(m, k, patch)) } else texteGlisse = { k, patch }
  }
  function texteClavier(e: KeyboardEvent, k: number) {
    const d = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key]
    if (!d) return
    e.preventDefault()
    e.stopPropagation()
    const pasTexte = e.shiftKey ? 0.05 : 0.01
    modifier(reglerTexte(m, k, { x: m.textes[k].x + d[0] * pasTexte, y: m.textes[k].y + d[1] * pasTexte }))
  }
  function tirerIntervalle(e: PointerEvent, k: number, bord: Cote | null) {
    e.stopPropagation()
    if (e.type === 'pointerdown') { prise = { ...prise, borne: versMontage(m, axeA(e)), debut: m.textes[k].debut, fin: m.textes[k].fin }; texteChoisi = k }
    if (!capture(e) || !ligne) return
    const instant = versMontage(m, axeA(e))
    const ecart = Math.min(dureeFinale(m) - prise.fin, Math.max(-prise.debut, instant - prise.borne))
    const patch = bord === 'debut' ? { debut: Math.min(instant, prise.fin - 0.1) } : bord === 'fin' ? { fin: Math.max(instant, prise.debut + 0.1) } : { debut: prise.debut + ecart, fin: prise.fin + ecart }
    if (e.type === 'pointerup') { texteGlisse = null; modifier(reglerTexte(m, k, patch)) } else texteGlisse = { k, patch }
  }
  function nouveauTexte() {
    if (!modifier(ajouterTexte(m, temps))) return
    recadrage = false
    texteChoisi = m.textes.length - 1
  }
  function supprimerTexte() {
    modifier(retirerTexte(m, texteChoisi))
    texteChoisi = -1
  }
  const reglerLeTexte = (patch: Partial<Texte>) => modifier(reglerTexte(m, texteChoisi, patch))
  async function ajouter(c: (typeof disponibles)[number]) {
    ajout = false
    await ajouterAuMontage(c)
  }
  function lancer() {
    sortie = false
    void exporter()
  }
</script>

<svelte:window onkeydown={clavier} />

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <button class="v-ib" onclick={fermerEditeur} aria-label="Retour aux clips" title="Retour aux clips"><Icon name="gauche" /></button>
  <div class="main-head-t"><h1 class="truncate" title={m.nom}>{m.nom}</h1><span class="plbls-badge">Montage</span></div>
  <div class="main-head-actions">
    <button class="v-ib" onclick={ACTIONS.annuler} disabled={!editeur.historique?.passe.length} aria-label="Annuler" title="Annuler (Ctrl + Z)"><Icon name="annule" /></button>
    <button class="v-ib" onclick={ACTIONS.retablir} disabled={!editeur.historique?.futur.length} aria-label="Rétablir" title="Rétablir (Ctrl + Y)"><Icon name="retablis" /></button>
    <button class="plbls-button plbls-button--compact" onclick={() => (sortie = true)} disabled={!!travail || finale <= 0}><Icon name="telecharge" size={16} />Exporter</button>
  </div>
</header>
<div class="ed-page" bind:this={racine}>
  {#if travail}
    <div class="v-alert ed-export" role="status">
      <div class="plbls-progress grow">
        <div class="plbls-progress__entete"><span>{travail.libelle}</span><span>{travail.pct} %</span></div>
        <div class="plbls-progress__piste" role="progressbar" aria-label={travail.libelle} aria-valuemin="0" aria-valuemax="100" aria-valuenow={travail.pct}><div class="plbls-progress__remplissage" style:width="{travail.pct}%"></div></div>
      </div>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={travail.annuler}>Annuler l’export</button>
    </div>
  {/if}
  {#if editeur.preparation}
    <div class="v-alert" role="status"><Icon name="ciseaux" size={18} /><span>Préparation de « {editeur.preparation.nom} »{editeur.preparation.pct === null ? '…' : ` : ${editeur.preparation.pct} %`}</span></div>
  {/if}
  {#if editeur.message}
    <div class="v-alert {editeur.message.genre}" role={editeur.message.genre === 'dgr' ? 'alert' : 'status'}>
      <Icon name={editeur.message.genre === 'ok' ? 'ok' : 'alerte'} size={18} /><span class="grow">{editeur.message.texte}</span>
      <button class="v-ib sm" aria-label="Fermer le message" onclick={() => (editeur.message = null)}><Icon name="x" size={16} /></button>
    </div>
  {/if}
  <div class="ed-clips" role="list" aria-label="Clips du montage">
    {#each ordre as nom, k (nom)}
      <div class="ed-clip" class:actif={nom === segCible.source} role="listitem" draggable="true" ondragstart={(e) => saisir(e, 'clip', k)} ondragover={(e) => survoler(e, 'clip')} ondrop={(e) => deposer(e, 'clip', k)}>
        <Icon name="film" size={16} /><span class="truncate" title={sansExtension(nom)}>{sansExtension(nom)}</span><span class="v-mono v-mute">{dureeClip(sources[nom].dureeS)}</span>
        <button class="v-ib sm" aria-label="Déplacer {sansExtension(nom)} vers le début" title="Déplacer vers le début" disabled={k === 0} onclick={() => changer(deplacerClip(m, nom, k - 1))}><Icon name="gauche" size={14} /></button>
        <button class="v-ib sm" aria-label="Déplacer {sansExtension(nom)} vers la fin" title="Déplacer vers la fin" disabled={k === ordre.length - 1} onclick={() => changer(deplacerClip(m, nom, k + 1))}><Icon name="droite" size={14} /></button>
        <button class="v-ib sm" aria-label="Retirer {sansExtension(nom)} du montage" title="Retirer du montage (le fichier reste dans la galerie)" disabled={ordre.length === 1} onclick={() => changer(retirerClip(m, nom))}><Icon name="x" size={14} /></button>
      </div>
    {/each}
    <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (ajout = true)} disabled={!!editeur.preparation}><Icon name="plus" size={16} />Ajouter un clip</button>
  </div>
  <div class="ed-ecran">
    <div class="ed-cadre" bind:this={ecran} style:width="min(100%, calc(38vh * {recadrage ? format.toile[0] / format.toile[1] : format.largeur / format.hauteur}))" style:aspect-ratio="{recadrage ? format.toile[0] : format.largeur} / {recadrage ? format.toile[1] : format.hauteur}">
      <div class="ed-toile" style:left={recadrage ? '0' : pourcent(-format.cadre[2] / format.cadre[0])} style:top={recadrage ? '0' : pourcent(-format.cadre[3] / format.cadre[1])}
        style:width={recadrage ? '100%' : pourcent(format.toile[0] / format.cadre[0])} style:height={recadrage ? '100%' : pourcent(format.toile[1] / format.cadre[1])}>
        {#each clips as c (c.source)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video bind:this={videos[c.source]} class:ed-video={c.source === seg.source} hidden={c.source !== seg.source || illisibles[c.source]} muted playsinline preload="auto" src={urlFichier(c.apercu)}
            onerror={() => (illisibles[c.source] = true)} onloadedmetadata={(e) => evenement(e, () => placer({ i, t }))} onclick={basculerLecture}
            onplay={(e) => evenement(e, demarre)} onpause={(e) => evenement(e, () => { lecture = false; arreterSon() })} onplaying={(e) => evenement(e, (v) => demarrerSon(v.currentTime))}
            onwaiting={(e) => evenement(e, arreterSon)} onseeking={(e) => evenement(e, arreterSon)} onratechange={(e) => evenement(e, (v) => !v.paused && !v.seeking && demarrerSon(v.currentTime))}
            onseeked={(e) => evenement(e, (v) => !v.paused && demarrerSon(v.currentTime))} ontimeupdate={(e) => evenement(e, (v) => !v.paused && !v.seeking && !parImage && suivre(v.currentTime))}></video>
        {/each}
        {#if illisibles[seg.source]}<p class="ed-illisible" role="alert">Le lecteur intégré ne sait pas lire l’aperçu de ce clip. Le montage et l’export restent possibles, sans image.</p>{/if}
      </div>
      {#each visibles as k (k)}
        <canvas class="ed-texte" use:peindre={{ t: textes[k], k, l: format.largeur, h: format.hauteur }}></canvas>
        {#if boites[k]}
          <div class="ed-prise" class:choisie={texteChoisi === k} role="slider" tabindex="0" aria-label="Position du texte « {textes[k].contenu} »" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(textes[k].x * 100)}
            aria-valuetext="{Math.round(textes[k].x * 100)} % depuis la gauche, {Math.round(textes[k].y * 100)} % depuis le haut" style:left={pourcent(boites[k].x)} style:top={pourcent(boites[k].y)} style:width={pourcent(boites[k].l)} style:height={pourcent(boites[k].h)}
            onpointerdown={(e) => tirerTexte(e, k)} onpointermove={(e) => tirerTexte(e, k)} onpointerup={(e) => tirerTexte(e, k)} onkeydown={(e) => texteClavier(e, k)}></div>
        {/if}
      {/each}
      {#if recadrage}
        <div class="ed-rogne" role="slider" tabindex="0" aria-label="Cadre de sortie" aria-valuemin="5" aria-valuemax="100" aria-valuenow={Math.round(cadre.l * 100)}
          aria-valuetext="{Math.round(cadre.l * 100)} % de la largeur, {Math.round(cadre.h * 100)} % de la hauteur, à {Math.round(cadre.x * 100)} % de la gauche et {Math.round(cadre.y * 100)} % du haut"
          style:left={pourcent(cadre.x)} style:top={pourcent(cadre.y)} style:width={pourcent(cadre.l)} style:height={pourcent(cadre.h)}
          onpointerdown={(e) => tirerCadre(e, false)} onpointermove={(e) => tirerCadre(e, false)} onpointerup={(e) => tirerCadre(e, false)} onkeydown={cadreClavier}>
          <span class="ed-coin" role="presentation" onpointerdown={(e) => tirerCadre(e, true)} onpointermove={(e) => tirerCadre(e, true)} onpointerup={(e) => tirerCadre(e, true)}></span>
        </div>
      {/if}
    </div>
  </div>
  <div class="ed-barre">
    <div class="ed-groupe">
      <button class="v-ib" onclick={ACTIONS.precedente} aria-label="Image précédente" title="Image précédente (←)"><Icon name="precedent" /></button>
      <button class="v-ib lg" onclick={basculerLecture} disabled={illisibles[seg.source] || finale <= 0} aria-label={lecture ? 'Pause' : 'Lecture'} title="{lecture ? 'Pause' : 'Lecture'} (Espace)"><Icon name={lecture ? 'pause' : 'lecture'} size={20} /></button>
      <button class="v-ib" onclick={ACTIONS.suivante} aria-label="Image suivante" title="Image suivante (→)"><Icon name="suivant" /></button>
      <span class="ed-temps v-mono" aria-live="off"><b>{formaterTemps(versMontage(vue, u))}</b> / <span title="Durée finale après montage">{formaterTemps(finale)}</span></span>
    </div>
    <div class="ed-groupe">
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={couperIci} disabled={couper(m, sources[seg.source], t, i) === m} title="Poser un point de coupe (C)"><Icon name="ciseaux" size={16} />Couper ici</button>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={basculerSegment} title="Retirer ou restaurer le segment (Suppr)">
        <Icon name={segCible.garde ? 'corbeille' : 'actualise'} size={16} />{segCible.garde ? 'Retirer le segment' : 'Restaurer le segment'}
      </button>
      <button class="plbls-button plbls-button--compact plbls-button--contour" aria-pressed={recadrage} onclick={() => { recadrage = !recadrage; cadreGlisse = null }}><Icon name="recadre" size={16} />{recadrage ? 'Terminer le recadrage' : 'Recadrer'}</button>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={nouveauTexte}><Icon name="plus" size={16} />Ajouter un texte</button>
    </div>
    <div class="ed-groupe ed-saisies">
      <label>Position<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(temps)} onchange={(e) => saisie(e, (x) => aller(versAxe(m, x)), () => temps)} /></label>
      <label>Début<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(b.debut)} onchange={(e) => saisie(e, (x) => changer(rogner(m, srcCible, 'debut', x, cible), debutBloc), () => bornes(m, cible).debut)} /></label>
      <label>Fin<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(b.fin)} onchange={(e) => saisie(e, (x) => changer(rogner(m, srcCible, 'fin', x, cible), debutBloc), () => bornes(m, cible).fin)} /></label>
    </div>
  </div>
  {#if recadrage}
    <div class="ed-panneau" role="group" aria-label="Recadrage">
      <label>Rapport
        <select class="plbls-select" value={m.cadre.rapport} onchange={(e) => modifier(recadrer(m, cadreAu(e.currentTarget.value as Rapport, m.cadre, format.toile), format.toile))}>
          {#each RAPPORTS as r (r)}<option value={r}>{r === 'libre' ? 'Libre' : r}</option>{/each}
        </select>
      </label>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => modifier(recadrer(m, CADRE_PLEIN, format.toile))}>Image entière</button>
      <span class="v-note">Sortie : {format.largeur} × {format.hauteur}. Le cadre vaut pour tout le montage ; glisse-le, tire son coin, ou règle-le aux flèches (Maj pour la taille).</span>
    </div>
  {:else if texte}
    <div class="ed-panneau" role="group" aria-label="Bloc de texte">
      <label class="ed-large">Texte<textarea class="plbls-textarea" rows="2" maxlength={TEXTE_MAX} value={texte.contenu} onchange={(e) => { reglerLeTexte({ contenu: e.currentTarget.value }); e.currentTarget.value = m.textes[texteChoisi]?.contenu ?? '' }}></textarea></label>
      <label>Police
        <select class="plbls-select" value={texte.police} onchange={(e) => reglerLeTexte({ police: e.currentTarget.value })}>{#each POLICES as p (p)}<option value={p}>{p}</option>{/each}</select>
      </label>
      <label>Taille<input type="range" min="2" max="40" step="1" value={Math.round(texte.taille * 100)} aria-valuetext="{Math.round(texte.taille * 100)} % du petit côté de l’image" oninput={(e) => (texteGlisse = { k: texteChoisi, patch: { taille: Number(e.currentTarget.value) / 100 } })}
        onchange={(e) => { texteGlisse = null; reglerLeTexte({ taille: Number(e.currentTarget.value) / 100 }) }} /></label>
      <label>Couleur<input type="color" value={texte.couleur} onchange={(e) => reglerLeTexte({ couleur: e.currentTarget.value })} /></label>
      <label>Lisibilité
        <select class="plbls-select" value={texte.habillage} onchange={(e) => reglerLeTexte({ habillage: e.currentTarget.value as Habillage })}>{#each HABILLAGES as [id, nom] (id)}<option value={id}>{nom}</option>{/each}</select>
      </label>
      <label>Apparaît à<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(texte.debut)} onchange={(e) => saisie(e, (x) => reglerLeTexte({ debut: Math.min(x, texte.fin - 0.1) }), () => m.textes[texteChoisi]?.debut ?? 0)} /></label>
      <label>Disparaît à<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(texte.fin)} onchange={(e) => saisie(e, (x) => reglerLeTexte({ fin: x }), () => m.textes[texteChoisi]?.fin ?? 0)} /></label>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={supprimerTexte}><Icon name="corbeille" size={16} />Supprimer le texte</button>
      <span class="v-note">Glisse le texte dans l’aperçu pour le placer. Accents et émojis sont exportés tels qu’affichés ici.</span>
    </div>
  {:else}
    <div class="ed-panneau" role="group" aria-label="Segment choisi">
      <span class="ed-quoi truncate" title={sansExtension(segCible.source)}>Segment {cible + 1} · {sansExtension(segCible.source)}</span>
      <label>Vitesse
        <select class="plbls-select" value={segCible.vitesse} onchange={(e) => changer(reglerSegment(m, cible, { vitesse: Number(e.currentTarget.value) }), cible)}>
          {#each VITESSES as v (v)}<option value={v}>{String(v).replace('.', ',')}×</option>{/each}
        </select>
      </label>
      <button class="plbls-button plbls-button--compact plbls-button--contour" aria-pressed={segCible.muet} onclick={() => modifier(reglerSegment(m, cible, { muet: !segCible.muet }))}><Icon name={segCible.muet ? 'volcoupe' : 'vol'} size={16} />{segCible.muet ? 'Son du segment coupé' : 'Couper le son du segment'}</button>
      <button class="v-ib" aria-label="Déplacer le segment vers le début" title="Déplacer le segment vers le début" disabled={cible === 0} onclick={() => changer(deplacer(m, cible, cible - 1), cible - 1)}><Icon name="gauche" /></button>
      <button class="v-ib" aria-label="Déplacer le segment vers la fin" title="Déplacer le segment vers la fin" disabled={cible === m.segments.length - 1} onclick={() => changer(deplacer(m, cible, cible + 1), cible + 1)}><Icon name="droite" /></button>
      {#if segCible.garde && saccade(segCible, srcCible, format.ips)}
        <span class="v-tag att ed-saccade" role="status"><Icon name="alerte" size={14} />Saccades : {srcCible.ips} i/s à {String(segCible.vitesse).replace('.', ',')}× ne donnent que {Math.round(srcCible.ips * segCible.vitesse)} images par seconde pour une sortie à {format.ips} i/s.</span>
      {/if}
    </div>
  {/if}
  <div class="ed-table">
    <div class="ed-noms">
      <div class="ed-nom ed-h-regle v-note">Montage : {formaterTemps(temps)} / {formaterTemps(finale)}</div>
      <div class="ed-nom ed-h-piste"><Icon name="film" size={16} /><span class="grow truncate">Vidéo</span></div>
      {#each { length: nombrePistes } as _, k (k)}
        {@const p = clipCible.pistes[k]}
        {@const v = volume(k)}
        {@const niveau = Math.round((ecoute[k] ?? v.volume) * 100)}
        <div class="ed-nom ed-h-piste">
          {#if p}
            <button class="v-ib sm" aria-pressed={v.muet} aria-label="Couper la piste {p.nom}" title={v.muet ? 'Rétablir le son de la piste' : 'Couper le son de la piste'} onclick={() => modifier(regler(m, srcCible, k, { muet: !v.muet }))}><Icon name={v.muet ? 'volcoupe' : 'vol'} size={16} /></button>
            <span class="ed-piste truncate" title="{p.nom} de {sansExtension(segCible.source)}">{p.nom}</span>
            <input type="range" min="0" max="200" step="5" value={niveau} disabled={v.muet} aria-label="Volume de la piste {p.nom}" aria-valuetext="{niveau} %"
              oninput={(e) => (ecoute[k] = Number(e.currentTarget.value) / 100)} onchange={(e) => reglerVolume(k, Number(e.currentTarget.value))} />
            <span class="ed-pct v-mono">{v.muet ? 'muet' : `${niveau} %`}</span>
          {:else}
            <Icon name="volcoupe" size={16} /><span class="v-note truncate">Pas de piste {k === 1 ? 'micro' : k + 1} dans ce clip</span>
          {/if}
        </div>
      {/each}
      {#each textes as x, k (k)}
        <div class="ed-nom ed-h-texte">
          <button class="ed-nom-texte truncate" aria-pressed={texteChoisi === k} title={x.contenu} onclick={() => choisirTexte(k)}><Icon name="crayon" size={14} /><span class="truncate">{x.contenu}</span></button>
        </div>
      {/each}
    </div>
    <div class="ed-ligne" bind:this={ligne}>
      <div class="ed-regle ed-h-regle" role="slider" tabindex="0" aria-label="Position dans le montage" aria-valuemin="0" aria-valuemax={finale} aria-valuenow={temps} aria-valuetext={formaterTemps(temps)}
        onpointerdown={tete} onpointermove={tete} onkeydown={teteClavier}>
        <span class="v-mono">0:00</span><span class="v-mono">{formaterTemps(finale)}</span>
      </div>
      <div class="ed-segments ed-h-piste">
        {#each vue.segments as s, j (j)}
          <button class="ed-segment" class:retire={!s.garde} class:raccord={j > 0 && vue.segments[j - 1].source !== s.source} aria-pressed={!glisse && cible === j} style:left={pc(ligneTemps.debuts[j])} style:width={pc(s.fin - s.debut)}
            draggable="true" ondragstart={(e) => saisir(e, 'segment', j)} ondragover={(e) => survoler(e, 'segment')} ondrop={(e) => deposer(e, 'segment', j)} onclick={() => choisir(j)}
            aria-label="Segment {j + 1}, {sansExtension(s.source)}, de {formaterTemps(s.debut)} à {formaterTemps(s.fin)}, {s.garde ? 'gardé' : 'retiré'}{s.vitesse === 1 ? '' : `, vitesse ${String(s.vitesse).replace('.', ',')}×`}{s.muet ? ', son coupé' : ''}"
            title="{sansExtension(s.source)} : {formaterTemps(s.debut)} – {formaterTemps(s.fin)}{s.garde ? '' : ' (retiré)'}">{#if s.vitesse !== 1}<span class="v-mono">{String(s.vitesse).replace('.', ',')}×</span>{/if}{#if s.muet}<Icon name="volcoupe" size={12} />{/if}</button>
        {/each}
        {#each ['debut', 'fin'] as const as cote (cote)}
          <div class="ed-poignee {cote}" role="slider" tabindex="0" aria-label={cote === 'debut' ? 'Début gardé' : 'Fin gardée'} aria-valuemin="0" aria-valuemax={srcCible.dureeS} aria-valuenow={b[cote]} aria-valuetext={formaterTemps(b[cote])}
            style:left={pc(ligneTemps.debuts[debutBloc] + (cote === 'fin' ? b.fin - b.debut : 0))} onpointerdown={(e) => poignee(e, cote)} onpointermove={(e) => poignee(e, cote)} onpointerup={(e) => poignee(e, cote)} onkeydown={(e) => poigneeClavier(e, cote)}></div>
        {/each}
      </div>
      {#each { length: nombrePistes } as _, k (k)}
        <div class="ed-onde ed-h-piste" role="presentation" onpointerdown={tete} onpointermove={tete}>
          {#each vue.segments as s, j (j)}
            {@const entier = duree(sources[s.source])}
            {#if ondes[s.source]?.[k]}
              <svg class:muette={m.volumes[s.source]?.[k]?.muet || s.muet} viewBox="{(s.debut / entier) * POINTS} 0 {((s.fin - s.debut) / entier) * POINTS} 100" preserveAspectRatio="none" aria-hidden="true" style:left={pc(ligneTemps.debuts[j])} style:width={pc(s.fin - s.debut)}><path d={ondes[s.source][k]} /></svg>
            {:else if sources[s.source].pistes > k && son[s.source] !== 'pret'}
              <span class="v-note truncate" style:left={pc(ligneTemps.debuts[j])} style:width={pc(s.fin - s.debut)}>{son[s.source] === 'erreur' ? 'Son illisible : aperçu muet, l’export garde le son.' : 'Préparation du son…'}</span>
            {/if}
          {/each}
        </div>
      {/each}
      {#each textes as x, k (k)}
        {@const gauche = versAxe(vue, x.debut)}
        <div class="ed-h-texte ed-intervalles">
          <div class="ed-intervalle" class:choisi={texteChoisi === k} role="slider" tabindex="0" aria-label="Intervalle du texte « {x.contenu} »" aria-valuemin="0" aria-valuemax={finale} aria-valuenow={x.debut} aria-valuetext="de {formaterTemps(x.debut)} à {formaterTemps(x.fin)}"
            style:left={pc(gauche)} style:width={pc(Math.max(0, versAxe(vue, Math.min(x.fin, finale)) - gauche))} onpointerdown={(e) => tirerIntervalle(e, k, null)} onpointermove={(e) => tirerIntervalle(e, k, null)} onpointerup={(e) => tirerIntervalle(e, k, null)}
            onkeydown={(e) => { const d = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0; if (!d) return; e.preventDefault(); e.stopPropagation(); const ecart = Math.min(finale - x.fin, Math.max(-x.debut, d * (e.shiftKey ? 1 : 0.1))); texteChoisi = k; modifier(reglerTexte(m, k, { debut: x.debut + ecart, fin: x.fin + ecart })) }}>
            {#each ['debut', 'fin'] as const as bord (bord)}<span class="ed-bord {bord}" role="presentation" onpointerdown={(e) => tirerIntervalle(e, k, bord)} onpointermove={(e) => tirerIntervalle(e, k, bord)} onpointerup={(e) => tirerIntervalle(e, k, bord)}></span>{/each}
          </div>
        </div>
      {/each}
      {#each vue.segments as s, j (j)}{#if !s.garde}<div class="ed-hors" style:left={pc(ligneTemps.debuts[j])} style:width={pc(s.fin - s.debut)} style:height="calc(var(--piste) * {nombrePistes})"></div>{/if}{/each}
      <div class="ed-tete" style:left={pc(u)}></div>
    </div>
  </div>
  <ul class="ed-raccourcis v-note" aria-label="Raccourcis clavier">
    {#each RACCOURCIS as r (r.libelle)}<li>{#each r.touches as k (k)}<kbd class="v-kbd">{k}</kbd>{/each}<span>{r.libelle}</span></li>{/each}
  </ul>
</div>

{#if ajout}
  <Modal title="Ajouter un clip au montage" onclose={() => (ajout = false)}>
    {#if disponibles.length}
      <ul class="ed-choix">
        {#each disponibles as c (c.chemin)}
          <li><button class="ed-choix-clip" onclick={() => ajouter(c)}><Icon name="film" size={18} /><span class="grow truncate">{c.nom}</span><span class="v-note">{[c.dureeS && dureeClip(c.dureeS), c.hauteur && `${c.hauteur}p`, c.ips && `${c.ips} i/s`, c.codec === 'hevc' && 'HEVC'].filter(Boolean).join(' · ')}</span></button></li>
        {/each}
      </ul>
    {:else}
      <p class="v-note">Tous les clips de la galerie sont déjà dans ce montage.</p>
    {/if}
  </Modal>
{/if}
{#if sortie}
  <Modal title="Exporter le montage" onclose={() => (sortie = false)}>
    <div class="ed-sortie">
      <label class="plbls-field"><span class="plbls-field__label">Résolution</span>
        <select class="plbls-select" value={m.sortie.hauteur ?? 0} onchange={(e) => modifier(reglerSortie(m, { hauteur: Number(e.currentTarget.value) || null }))}>
          <option value={0}>Celle du cadre ({formatDe(reglerSortie(m, { hauteur: null }), sources).largeur} × {formatDe(reglerSortie(m, { hauteur: null }), sources).hauteur})</option>
          {#each hauteursPossibles(format) as h (h)}<option value={h}>{h}p</option>{/each}
        </select>
      </label>
      <label class="plbls-field"><span class="plbls-field__label">Cadence</span>
        <select class="plbls-select" value={format.ips} onchange={(e) => modifier(reglerSortie(m, { ips: Number(e.currentTarget.value) }))}>
          {#each cadencesPossibles(m, sources) as c (c)}<option value={c}>{c} i/s</option>{/each}
        </select>
      </label>
      <p class="v-note">Nouveau fichier H.264 + AAC de {formaterTemps(finale)}, en {format.largeur} × {format.hauteur} à {format.ips} i/s. Les clips d’origine ne sont pas modifiés ; l’export continue si tu fermes l’éditeur.</p>
      <button class="plbls-button" onclick={lancer}><Icon name="telecharge" size={16} />Lancer l’export</button>
    </div>
  </Modal>
{/if}

<style>
  .ed-page { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 14px; padding: 20px 24px 24px; scrollbar-width: thin; }
  .ed-page > * { flex-shrink: 0; }
  .ed-page .v-alert { align-items: center; }
  .ed-clips { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .ed-clip { display: flex; align-items: center; gap: 6px; max-width: 340px; min-width: 0; padding: 2px 4px 2px 10px; border-radius: var(--r-p); background: var(--papier-2); color: var(--encre-2); font-size: 13px; cursor: grab; }
  .ed-clip.actif { background: var(--rose); color: var(--encre); }
  .ed-ecran { display: grid; place-items: center; min-height: 160px; border-radius: var(--r-l); background: #000; overflow: hidden; }
  .ed-cadre { position: relative; overflow: hidden; background: #000; touch-action: none; user-select: none; }
  .ed-toile { position: absolute; }
  .ed-toile video { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; cursor: pointer; }
  .ed-illisible { position: absolute; inset: 0; display: grid; place-items: center; margin: 0; padding: 16px; color: #fff; font-size: 13px; text-align: center; }
  .ed-texte { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; }
  .ed-prise { position: absolute; border-radius: var(--r-xs); cursor: move; touch-action: none; }
  .ed-prise:hover, .ed-prise.choisie { box-shadow: 0 0 0 1px #fff, 0 0 0 2px #000; }
  .ed-prise:focus-visible { outline: none; box-shadow: 0 0 0 2px #fff, 0 0 0 4px var(--rose-fonce); }
  .ed-rogne { position: absolute; box-sizing: border-box; border: 2px solid #fff; box-shadow: 0 0 0 9999px rgb(0 0 0 / 60%); cursor: move; touch-action: none; }
  .ed-rogne:focus-visible { outline: none; border-color: var(--rose-fonce); }
  .ed-coin { position: absolute; right: 2px; bottom: 2px; width: 18px; height: 18px; border: 2px solid #000; border-radius: 50%; background: #fff; cursor: nwse-resize; touch-action: none; }
  .ed-barre, .ed-panneau { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 20px; }
  .ed-panneau { gap: 10px 16px; padding: 10px 14px; border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); }
  .ed-panneau label { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--gris); }
  .ed-panneau .plbls-select { min-height: 32px; }
  .ed-panneau .plbls-input { width: 92px; min-height: 32px; padding: 4px 8px; font-size: 13px; }
  .ed-panneau .ed-large { flex: 1 1 260px; }
  .ed-panneau textarea { flex: 1; min-height: 44px; padding: 6px 10px; font-size: 13px; resize: vertical; }
  .ed-panneau input[type='color'] { width: 36px; height: 32px; padding: 0; border: 1px solid var(--trait-fort); border-radius: var(--r-xs); background: none; }
  .ed-quoi { max-width: 260px; font-size: 13px; font-weight: 600; }
  .ed-saccade { height: auto; min-height: 20px; padding: 3px 7px; white-space: normal; }
  .ed-groupe { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .ed-temps { margin-left: 8px; font-size: 14px; color: var(--gris); white-space: nowrap; }
  .ed-temps b { color: var(--encre); }
  .ed-saisies { margin-left: auto; gap: 12px; }
  .ed-saisies label { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--gris); }
  .ed-saisies input { width: 92px; min-height: 32px; padding: 4px 8px; font-size: 13px; }
  .ed-table { display: grid; grid-template-columns: minmax(220px, 280px) minmax(320px, 1fr); border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); overflow-x: auto; }
  .ed-noms { display: flex; flex-direction: column; border-right: 1px solid var(--trait); }
  .ed-nom { display: flex; align-items: center; gap: 8px; padding: 0 12px; min-width: 0; font-size: 13px; }
  .ed-table { --regle: 28px; --piste: 52px; --texte: 30px; }
  .ed-h-regle { height: var(--regle); }
  .ed-h-piste { height: var(--piste); border-top: 1px solid var(--trait); }
  .ed-h-texte { height: var(--texte); border-top: 1px solid var(--trait); }
  .ed-piste { width: 44px; flex: none; }
  .ed-nom input[type='range'] { flex: 1; min-width: 60px; }
  .ed-pct { width: 46px; flex: none; text-align: right; color: var(--gris); }
  .ed-nom-texte { display: flex; align-items: center; gap: 6px; min-width: 0; max-width: 100%; padding: 2px 8px; border: 0; border-radius: var(--r-xs); background: none; color: inherit; font: inherit; cursor: pointer; }
  .ed-nom-texte[aria-pressed='true'] { background: var(--rose); color: var(--encre); }
  .ed-nom-texte:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .ed-ligne { position: relative; min-width: 0; margin: 0 10px; touch-action: none; user-select: none; }
  .ed-regle { display: flex; align-items: center; justify-content: space-between; color: var(--gris); cursor: ew-resize; }
  .ed-regle:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .ed-segments, .ed-intervalles { position: relative; }
  .ed-segment { position: absolute; top: 8px; bottom: 8px; z-index: 1; display: flex; align-items: center; justify-content: center; gap: 4px; overflow: hidden; padding: 0; border: 1px solid var(--surface); border-radius: var(--r-xs); background: var(--rose); color: var(--encre); font-size: 11px; cursor: pointer; }
  .ed-segment.raccord { border-left: 3px solid var(--encre); }
  .ed-segment.retire { background: repeating-linear-gradient(135deg, var(--papier-3) 0 6px, var(--papier-2) 6px 12px); }
  .ed-segment[aria-pressed='true'] { box-shadow: 0 0 0 2px var(--encre); z-index: 2; }
  .ed-segment:focus-visible { outline: none; box-shadow: var(--anneau-in); z-index: 2; }
  .ed-poignee { position: absolute; top: 2px; bottom: 2px; z-index: 3; width: 12px; border-radius: var(--r-xs); background: var(--encre); cursor: ew-resize; touch-action: none; }
  .ed-poignee.debut { transform: translateX(-100%); }
  .ed-poignee:focus-visible { outline: none; box-shadow: var(--halo); }
  .ed-onde { position: relative; cursor: ew-resize; color: var(--encre-2); }
  .ed-onde svg, .ed-onde span { position: absolute; top: 6px; height: 40px; }
  .ed-onde span { display: flex; align-items: center; padding: 0 6px; }
  .ed-onde path { fill: none; stroke: currentColor; stroke-width: 1; vector-effect: non-scaling-stroke; }
  .ed-onde svg.muette { color: var(--gris-2); opacity: 0.5; }
  .ed-intervalle { position: absolute; top: 5px; bottom: 5px; min-width: 6px; border-radius: var(--r-xs); background: var(--encre-2); cursor: grab; touch-action: none; }
  .ed-intervalle.choisi { background: var(--rose-fonce); }
  .ed-intervalle:focus-visible { outline: none; box-shadow: var(--halo); }
  .ed-bord { position: absolute; top: 0; bottom: 0; width: 8px; cursor: ew-resize; touch-action: none; }
  .ed-bord.debut { left: 0; }
  .ed-bord.fin { right: 0; }
  .ed-hors { position: absolute; top: calc(var(--regle) + var(--piste)); background: color-mix(in srgb, var(--surface) 72%, transparent); pointer-events: none; }
  .ed-tete { position: absolute; top: 0; bottom: 0; z-index: 4; width: 2px; margin-left: -1px; background: var(--danger); pointer-events: none; }
  .ed-raccourcis { display: flex; flex-wrap: wrap; gap: 6px 18px; margin: 0; padding: 0; list-style: none; }
  .ed-raccourcis li { display: flex; align-items: center; gap: 4px; }
  .ed-raccourcis span { margin-left: 2px; }
  .ed-choix { display: flex; flex-direction: column; gap: 4px; max-height: 50vh; margin: 0; padding: 0; overflow: auto; list-style: none; }
  .ed-choix-clip { display: flex; align-items: center; gap: 10px; width: 100%; padding: 10px 12px; border: 0; border-radius: var(--r); background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .ed-choix-clip:hover, .ed-choix-clip:focus-visible { outline: none; background: var(--papier-2); }
  .ed-sortie { display: flex; flex-direction: column; align-items: flex-start; gap: 14px; }
  .ed-sortie .plbls-field { align-self: stretch; }
</style>
