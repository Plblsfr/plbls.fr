<script lang="ts">
  import { onDestroy } from 'svelte'
  import Icon from '../../../../web/estompe/src/components/Icon.svelte'
  import {
    actionClavier, bornes, caler, couper, duree, dureeFinale, formaterTemps, gain, garder, gardes, imageDe, images, lireTemps, pas, pics, RACCOURCIS, regler, rogner, segmentA, suite, tempsImage, traceOnde, versMontage,
    type Action, type Cote,
  } from './montage'
  import { defaire, editeur, exporter, fermerEditeur, modifier, refaire } from './montage.svelte'
  import { replay, urlFichier } from './replay.svelte'

  let { onmenu }: { onmenu: () => void } = $props()

  const o = editeur.ouverture!
  const src = editeur.source!
  const ips = src.ips
  const total = duree(src)
  const POINTS = 600
  const parImage = 'requestVideoFrameCallback' in HTMLVideoElement.prototype

  let racine = $state<HTMLElement>()
  let video = $state<HTMLVideoElement>()
  let t = $state(0)
  let lecture = $state(false)
  let illisible = $state(false)
  let choisi = $state(-1)
  let glisse = $state<{ cote: Cote; t: number } | null>(null)
  let ecoute = $state<Record<number, number>>({})
  let son = $state<'chargement' | 'pret' | 'erreur'>(o.pistes.length ? 'chargement' : 'pret')
  let ondes = $state.raw<string[]>(o.pistes.map(() => ''))

  const m = $derived(editeur.historique!.present)
  const vue = $derived(glisse ? rogner(m, src, glisse.cote, glisse.t) : m)
  const b = $derived(bornes(vue))
  const finale = $derived(dureeFinale(vue))
  const cible = $derived(choisi >= 0 && choisi < m.segments.length ? choisi : segmentA(m, t))
  const travail = $derived(replay.travaux.find((x) => x.source === o.chemin && x.pct !== undefined))
  const pc = (x: number) => `${(x / total) * 100}%`

  let ctx: AudioContext | null = null
  let tampons: AudioBuffer[] = []
  let gains: GainNode[] = []
  let sources: AudioBufferSourceNode[] = []
  let depart = { horloge: 0, t: 0 }
  let detruit = false

  function arreterSon() {
    for (const s of sources) { s.onended = null; s.stop(); s.disconnect() }
    sources = []
  }
  function demarrerSon(temps: number) {
    arreterSon()
    const c = ctx
    if (!c || son !== 'pret') return
    void c.resume()
    sources = tampons.flatMap((tampon, i) => {
      if (temps >= tampon.duration) return []
      const s = c.createBufferSource()
      s.buffer = tampon
      s.connect(gains[i])
      s.start(0, Math.max(0, temps))
      return [s]
    })
    depart = { horloge: c.currentTime, t: temps }
  }
  function recalerSon(temps: number) {
    if (ctx && sources.length && Math.abs(depart.t + ctx.currentTime - depart.horloge - temps) > 0.04) demarrerSon(temps)
  }
  async function chargerSon() {
    try {
      const c = new AudioContext()
      ctx = c
      gains = o.pistes.map(() => { const g = c.createGain(); g.connect(c.destination); return g })
      const lus = await Promise.all(o.pistes.map(async (p) => c.decodeAudioData(await (await fetch(urlFichier(p.chemin))).arrayBuffer())))
      if (detruit) return
      tampons = lus
      ondes = lus.map((x) => traceOnde(pics(Array.from({ length: x.numberOfChannels }, (_, i) => x.getChannelData(i)), Math.max(1, Math.round((POINTS * Math.min(x.duration, total)) / total)))))
      son = 'pret'
      if (video && !video.paused) demarrerSon(video.currentTime)
    } catch {
      son = 'erreur'
    }
  }
  if (o.pistes.length) void chargerSon()
  $effect(() => {
    gains.forEach((g, i) => { g.gain.value = ecoute[i] ?? gain(m, src, i) })
  })
  onDestroy(() => {
    detruit = true
    arreterSon()
    void ctx?.close()
  })

  function aller(temps: number) {
    const i = Math.min(images(src) - 1, Math.max(0, imageDe(temps, ips)))
    t = i / ips
    if (video) video.currentTime = tempsImage(i, ips)
  }
  function suivre(temps: number) {
    t = Math.min(images(src) - 1, imageDe(temps, ips)) / ips
    const s = suite(m, temps + 1 / ips, ips)
    if (!s) return recalerSon(temps)
    if (!video) return
    if ('fin' in s) video.pause()
    else video.currentTime = tempsImage(Math.round(s.aller * ips), ips)
  }
  function image(_: number, meta: { mediaTime: number }) {
    if (!video || video.paused) return
    suivre(meta.mediaTime)
    video.requestVideoFrameCallback(image)
  }
  function demarre() {
    lecture = true
    if (parImage) video?.requestVideoFrameCallback(image)
  }
  function basculerLecture() {
    if (!video || illisible) return
    if (!video.paused) return video.pause()
    const s = suite(m, t, ips)
    const premier = gardes(m)[0]
    if (!premier) return
    if (s) aller('fin' in s ? premier.debut : s.aller)
    video.play().catch(() => (lecture = false))
  }
  function image1(n: number) {
    video?.pause()
    aller(pas(t, n, src))
  }
  function structure(apres: typeof m) {
    choisi = -1
    modifier(apres)
  }
  const basculerSegment = () => { if (cible >= 0) modifier(garder(m, cible, !m.segments[cible].garde)) }
  const ACTIONS: Record<Action, () => void> = {
    lecture: basculerLecture, precedente: () => image1(-1), suivante: () => image1(1), couper: () => structure(couper(m, src, t)),
    annuler: () => { choisi = -1; defaire() }, retablir: () => { choisi = -1; refaire() }, basculer: basculerSegment,
  }
  function clavier(e: KeyboardEvent) {
    const cibleClavier = e.target as HTMLElement
    if (!racine || !(racine.contains(cibleClavier) || cibleClavier === document.body) || e.defaultPrevented) return
    const action = actionClavier(e, cibleClavier.matches('input:not([type="range"]), textarea, select, [contenteditable="true"]'))
    if (!action || (action === 'lecture' && cibleClavier.matches('button, a')) || (['lecture', 'precedente', 'suivante'].includes(action) && cibleClavier.matches('input[type="range"]'))) return
    e.preventDefault()
    ACTIONS[action]()
  }

  const tempsA = (e: PointerEvent, el: HTMLElement) => {
    const r = el.getBoundingClientRect()
    return Math.min(total, Math.max(0, ((e.clientX - r.left) / Math.max(1, r.width)) * total))
  }
  let ligne = $state<HTMLElement>()
  function tete(e: PointerEvent) {
    if (e.type === 'pointerdown') { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); video?.pause() }
    else if (!(e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId)) return
    if (ligne) aller(tempsA(e, ligne))
  }
  function poignee(e: PointerEvent, cote: Cote) {
    const el = e.currentTarget as HTMLElement
    if (e.type === 'pointerdown') el.setPointerCapture(e.pointerId)
    else if (!el.hasPointerCapture(e.pointerId)) return
    if (!ligne) return
    if (e.type === 'pointerup') {
      const fin = glisse
      glisse = null
      el.releasePointerCapture(e.pointerId)
      if (fin) structure(rogner(m, src, fin.cote, fin.t))
    } else glisse = { cote, t: tempsA(e, ligne) }
  }
  function poigneeClavier(e: KeyboardEvent, cote: Cote) {
    const actuel = bornes(m)[cote]
    const saut = e.shiftKey ? 1 : 1 / ips
    const vers = e.key === 'ArrowLeft' ? actuel - saut : e.key === 'ArrowRight' ? actuel + saut : e.key === 'Home' ? 0 : e.key === 'End' ? total : null
    if (vers === null) return
    e.preventDefault()
    e.stopPropagation()
    structure(rogner(m, src, cote, vers))
  }
  function teteClavier(e: KeyboardEvent) {
    const vers = e.key === 'ArrowLeft' ? pas(t, e.shiftKey ? -ips : -1, src) : e.key === 'ArrowRight' ? pas(t, e.shiftKey ? ips : 1, src) : e.key === 'Home' ? 0 : e.key === 'End' ? total : null
    if (vers === null) return
    e.preventDefault()
    e.stopPropagation()
    video?.pause()
    aller(vers)
  }
  function saisie(e: Event, appliquerTemps: (temps: number) => void, actuel: () => number) {
    const champ = e.currentTarget as HTMLInputElement
    const lu = lireTemps(champ.value)
    champ.setAttribute('aria-invalid', String(lu === null))
    if (lu === null) return
    appliquerTemps(caler(lu, src))
    champ.value = formaterTemps(actuel())
  }
  const volume = (i: number) => m.volumes[src.nom]?.[i] ?? { volume: 1, muet: false }
  function reglerVolume(i: number, pourcent: number) {
    delete ecoute[i]
    modifier(regler(m, src, i, { volume: pourcent / 100 }))
  }
</script>

<svelte:window onkeydown={clavier} />

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <button class="v-ib" onclick={fermerEditeur} aria-label="Retour aux clips" title="Retour aux clips"><Icon name="gauche" /></button>
  <div class="main-head-t"><h1 class="truncate" title={o.nom}>{o.nom}</h1><span class="plbls-badge">Montage</span></div>
  <div class="main-head-actions">
    <button class="v-ib" onclick={ACTIONS.annuler} disabled={!editeur.historique?.passe.length} aria-label="Annuler" title="Annuler (Ctrl + Z)"><Icon name="annule" /></button>
    <button class="v-ib" onclick={ACTIONS.retablir} disabled={!editeur.historique?.futur.length} aria-label="Rétablir" title="Rétablir (Ctrl + Y)"><Icon name="retablis" /></button>
    <button class="plbls-button plbls-button--compact" onclick={exporter} disabled={!!travail || finale <= 0}><Icon name="telecharge" size={16} />Exporter</button>
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
  {#if editeur.message}
    <div class="v-alert {editeur.message.genre}" role={editeur.message.genre === 'dgr' ? 'alert' : 'status'}>
      <Icon name={editeur.message.genre === 'ok' ? 'ok' : 'alerte'} size={18} /><span class="grow">{editeur.message.texte}</span>
      <button class="v-ib sm" aria-label="Fermer le message" onclick={() => (editeur.message = null)}><Icon name="x" size={16} /></button>
    </div>
  {/if}
  <div class="ed-ecran">
    {#if illisible}
      <div class="plbls-alert plbls-alert--erreur" role="alert">
        <p class="plbls-alert__titre">Aperçu impossible</p>
        <p class="plbls-alert__corps">Le lecteur intégré ne sait pas lire l’aperçu de ce clip. Le montage et l’export restent possibles, sans image.</p>
      </div>
    {:else}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video bind:this={video} class="ed-video" muted playsinline preload="auto" src={urlFichier(o.apercu)} onerror={() => (illisible = true)} onloadedmetadata={() => aller(bornes(m).debut)} onclick={basculerLecture}
        onplay={demarre} onpause={() => { lecture = false; arreterSon() }} onplaying={() => video && demarrerSon(video.currentTime)} onwaiting={arreterSon} onseeking={arreterSon}
        onseeked={() => video && !video.paused && demarrerSon(video.currentTime)} ontimeupdate={() => video && !video.paused && !parImage && suivre(video.currentTime)}></video>
    {/if}
  </div>
  <div class="ed-barre">
    <div class="ed-groupe">
      <button class="v-ib" onclick={ACTIONS.precedente} aria-label="Image précédente" title="Image précédente (←)"><Icon name="precedent" /></button>
      <button class="v-ib lg" onclick={basculerLecture} disabled={illisible || finale <= 0} aria-label={lecture ? 'Pause' : 'Lecture'} title="{lecture ? 'Pause' : 'Lecture'} (Espace)"><Icon name={lecture ? 'pause' : 'lecture'} size={20} /></button>
      <button class="v-ib" onclick={ACTIONS.suivante} aria-label="Image suivante" title="Image suivante (→)"><Icon name="suivant" /></button>
      <span class="ed-temps v-mono" aria-live="off"><b>{formaterTemps(versMontage(vue, t))}</b> / <span title="Durée finale après montage">{formaterTemps(finale)}</span></span>
    </div>
    <div class="ed-groupe">
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={ACTIONS.couper} disabled={couper(m, src, t) === m} title="Poser un point de coupe (C)"><Icon name="ciseaux" size={16} />Couper ici</button>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={basculerSegment} disabled={cible < 0} title="Retirer ou restaurer le segment (Suppr)">
        <Icon name={cible >= 0 && !m.segments[cible].garde ? 'actualise' : 'corbeille'} size={16} />{cible >= 0 && !m.segments[cible].garde ? 'Restaurer le segment' : 'Retirer le segment'}
      </button>
    </div>
    <div class="ed-groupe ed-saisies">
      <label>Position<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(t)} onchange={(e) => saisie(e, aller, () => t)} /></label>
      <label>Début<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(b.debut)} onchange={(e) => saisie(e, (x) => structure(rogner(m, src, 'debut', x)), () => bornes(m).debut)} /></label>
      <label>Fin<input class="plbls-input v-mono" inputmode="decimal" value={formaterTemps(b.fin)} onchange={(e) => saisie(e, (x) => structure(rogner(m, src, 'fin', x)), () => bornes(m).fin)} /></label>
    </div>
  </div>
  <div class="ed-table">
    <div class="ed-noms">
      <div class="ed-nom ed-h-regle v-note">Clip : {formaterTemps(t)} / {formaterTemps(total)}</div>
      <div class="ed-nom ed-h-piste"><Icon name="film" size={16} /><span class="grow truncate">Vidéo</span></div>
      {#each o.pistes as p, i (p.chemin)}
        {@const v = volume(i)}
        {@const pourcent = Math.round((ecoute[i] ?? v.volume) * 100)}
        <div class="ed-nom ed-h-piste">
          <button class="v-ib sm" aria-pressed={v.muet} aria-label="Couper la piste {p.nom}" title={v.muet ? 'Rétablir le son de la piste' : 'Couper le son de la piste'} onclick={() => modifier(regler(m, src, i, { muet: !v.muet }))}><Icon name={v.muet ? 'volcoupe' : 'vol'} size={16} /></button>
          <span class="ed-piste truncate" title={p.nom}>{p.nom}</span>
          <input type="range" min="0" max="200" step="5" value={pourcent} disabled={v.muet} aria-label="Volume de la piste {p.nom}" aria-valuetext="{pourcent} %"
            oninput={(e) => (ecoute[i] = Number(e.currentTarget.value) / 100)} onchange={(e) => reglerVolume(i, Number(e.currentTarget.value))} />
          <span class="ed-pct v-mono">{v.muet ? 'muet' : `${pourcent} %`}</span>
        </div>
      {/each}
    </div>
    <div class="ed-ligne" bind:this={ligne}>
      <div class="ed-regle ed-h-regle" role="slider" tabindex="0" aria-label="Position dans le clip" aria-valuemin="0" aria-valuemax={total} aria-valuenow={t} aria-valuetext={formaterTemps(t)}
        onpointerdown={tete} onpointermove={tete} onkeydown={teteClavier}>
        <span class="v-mono">0:00</span><span class="v-mono">{formaterTemps(total)}</span>
      </div>
      <div class="ed-segments ed-h-piste">
        {#each vue.segments as s, i (i)}
          <button class="ed-segment" class:retire={!s.garde} aria-pressed={!glisse && cible === i} style:left={pc(s.debut)} style:width={pc(s.fin - s.debut)} onclick={() => (choisi = i)}
            aria-label="Segment {i + 1}, de {formaterTemps(s.debut)} à {formaterTemps(s.fin)}, {s.garde ? 'gardé' : 'retiré'}" title="{formaterTemps(s.debut)} – {formaterTemps(s.fin)}{s.garde ? '' : ' (retiré)'}"></button>
        {/each}
        {#each ['debut', 'fin'] as const as cote (cote)}
          <div class="ed-poignee {cote}" role="slider" tabindex="0" aria-label={cote === 'debut' ? 'Début gardé' : 'Fin gardée'} aria-valuemin="0" aria-valuemax={total} aria-valuenow={b[cote]} aria-valuetext={formaterTemps(b[cote])}
            style:left={pc(b[cote])} onpointerdown={(e) => poignee(e, cote)} onpointermove={(e) => poignee(e, cote)} onpointerup={(e) => poignee(e, cote)} onkeydown={(e) => poigneeClavier(e, cote)}></div>
        {/each}
      </div>
      {#each o.pistes as p, i (p.chemin)}
        <div class="ed-onde ed-h-piste" class:muette={volume(i).muet} role="presentation" onpointerdown={tete} onpointermove={tete}>
          {#if ondes[i]}<svg viewBox="0 0 {POINTS} 100" preserveAspectRatio="none" aria-hidden="true"><path d={ondes[i]} /></svg>
          {:else}<span class="v-note">{son === 'erreur' ? 'Son de la piste illisible : l’aperçu est muet, l’export garde le son.' : 'Préparation du son…'}</span>{/if}
        </div>
      {/each}
      <div class="ed-hors" style:left="0" style:width={pc(b.debut)}></div>
      <div class="ed-hors" style:left={pc(b.fin)} style:right="0"></div>
      {#each vue.segments as s, i (i)}{#if !s.garde}<div class="ed-hors ed-retire" style:left={pc(s.debut)} style:width={pc(s.fin - s.debut)}></div>{/if}{/each}
      <div class="ed-tete" style:left={pc(t)}></div>
    </div>
  </div>
  <ul class="ed-raccourcis v-note" aria-label="Raccourcis clavier">
    {#each RACCOURCIS as r (r.libelle)}<li>{#each r.touches as k (k)}<kbd class="v-kbd">{k}</kbd>{/each}<span>{r.libelle}</span></li>{/each}
  </ul>
</div>

<style>
  .ed-page { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 14px; padding: 20px 24px 24px; scrollbar-width: thin; }
  .ed-page .v-alert { align-items: center; }
  .ed-ecran { display: grid; place-items: center; min-height: 160px; border-radius: var(--r-l); background: #000; overflow: hidden; }
  .ed-ecran .plbls-alert { margin: 16px; }
  .ed-video { display: block; width: 100%; max-height: 38vh; cursor: pointer; }
  .ed-barre { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 20px; }
  .ed-groupe { display: flex; align-items: center; gap: 6px; }
  .ed-temps { margin-left: 8px; font-size: 14px; color: var(--gris); white-space: nowrap; }
  .ed-temps b { color: var(--encre); }
  .ed-saisies { margin-left: auto; gap: 12px; }
  .ed-saisies label { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--gris); }
  .ed-saisies input { width: 92px; min-height: 32px; padding: 4px 8px; font-size: 13px; }
  .ed-table { display: grid; grid-template-columns: minmax(220px, 280px) minmax(320px, 1fr); border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); overflow-x: auto; }
  .ed-noms { display: flex; flex-direction: column; border-right: 1px solid var(--trait); }
  .ed-nom { display: flex; align-items: center; gap: 8px; padding: 0 12px; min-width: 0; font-size: 13px; }
  .ed-table { --regle: 28px; --piste: 52px; }
  .ed-h-regle { height: var(--regle); }
  .ed-h-piste { height: var(--piste); border-top: 1px solid var(--trait); }
  .ed-piste { width: 44px; flex: none; }
  .ed-nom input[type='range'] { flex: 1; min-width: 60px; }
  .ed-pct { width: 46px; flex: none; text-align: right; color: var(--gris); }
  .ed-ligne { position: relative; min-width: 0; margin: 0 10px; touch-action: none; user-select: none; }
  .ed-regle { display: flex; align-items: center; justify-content: space-between; color: var(--gris); cursor: ew-resize; }
  .ed-regle:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .ed-segments { position: relative; }
  .ed-segment { position: absolute; top: 8px; bottom: 8px; z-index: 1; padding: 0; border: 1px solid var(--surface); border-radius: var(--r-xs); background: var(--rose); cursor: pointer; }
  .ed-segment.retire { background: repeating-linear-gradient(135deg, var(--papier-3) 0 6px, var(--papier-2) 6px 12px); }
  .ed-segment[aria-pressed='true'] { box-shadow: 0 0 0 2px var(--encre); z-index: 2; }
  .ed-segment:focus-visible { outline: none; box-shadow: var(--anneau-in); z-index: 2; }
  .ed-poignee { position: absolute; top: 2px; bottom: 2px; z-index: 3; width: 12px; border-radius: var(--r-xs); background: var(--encre); cursor: ew-resize; touch-action: none; }
  .ed-poignee.debut { transform: translateX(-100%); }
  .ed-poignee:focus-visible { outline: none; box-shadow: var(--halo); }
  .ed-onde { display: flex; align-items: center; cursor: ew-resize; color: var(--encre-2); }
  .ed-onde svg { width: 100%; height: 40px; }
  .ed-onde path { fill: none; stroke: currentColor; stroke-width: 1; vector-effect: non-scaling-stroke; }
  .ed-onde.muette { color: var(--gris-2); opacity: 0.5; }
  .ed-hors { position: absolute; top: var(--regle); bottom: 0; background: color-mix(in srgb, var(--surface) 72%, transparent); pointer-events: none; }
  .ed-retire { top: calc(var(--regle) + var(--piste)); }
  .ed-tete { position: absolute; top: 0; bottom: 0; z-index: 4; width: 2px; margin-left: -1px; background: var(--danger); pointer-events: none; }
  .ed-raccourcis { display: flex; flex-wrap: wrap; gap: 6px 18px; margin: 0; padding: 0; list-style: none; }
  .ed-raccourcis li { display: flex; align-items: center; gap: 4px; }
  .ed-raccourcis span { margin-left: 2px; }
</style>
