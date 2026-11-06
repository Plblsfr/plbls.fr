<script lang="ts">
  import { confirmer, saisir } from '../../frontend/src/lib/confirm.svelte'
  import { onMount } from 'svelte'
  import ReplaySend from './ReplaySend.svelte'
  import { sends } from './replaySend.svelte'
  import { audioMix, audioTracks, clampCrop, clipAround, cropForRatio, estimateSize, formatDuration, formatSize, formatTimecode, frameDuration, markerCount, markerName, onExport, parseTimecode, planLabel, replayUrl, replaysCancel, replaysCopy, replaysDelete, replaysEncoders, replaysExport, replaysKeyframes, replaysList, replaysPlan, replaysMarkersSet, replaysRename, replaysReveal, replaysThumb, replaysTracks, resizeCrop, snapToKeyframe, trackVolume, visible, withMarker, type AudioTrack, type Crop, type ExportParams, type ExportPlan, type Library, type Marker, type Quality, type Replay, type Sort } from './replays'
  import { messageOf, notify } from '../../frontend/src/lib/ui'

  const PAGE = 48
  const RATIOS = [['Libre', 0], ['16:9', 16 / 9], ['9:16', 9 / 16], ['1:1', 1]] as const
  const SPEEDS = [0.25, 0.5, 1, 1.5, 2]

  let lib = $state<Library>()
  let error = $state('')
  let query = $state('')
  let game = $state('')
  let sort = $state<Sort>('date')
  let shown = $state(PAGE)
  let thumbs = $state<Record<string, string>>({})
  let current = $state<Replay | null>(null)
  let sending = $state<Replay | null>(null)
  let exported = $state('')
  let selected = $state<string[]>([])

  const games = $derived([...new Set((lib?.replays ?? []).map((r) => r.game).filter(Boolean))].sort())
  const list = $derived(visible(lib?.replays ?? [], query, game, sort))
  const used = $derived((lib?.replays ?? []).reduce((n, r) => n + r.size, 0))

  async function refresh() {
    try {
      lib = await replaysList()
      error = ''
      selected = selected.filter((n) => lib!.replays.some((r) => r.name === n))
      if (current && !lib.replays.some((r) => r.name === current!.name)) current = null
    } catch (e) {
      error = messageOf(e)
    }
  }

  // Les fichiers ajoutés ou supprimés hors d'Estompe : la liste est relue toutes les 3 s, et au retour sur la fenêtre.
  onMount(() => {
    void refresh()
    void replaysEncoders().then((l) => (encoders = l), () => {})
    const t = setInterval(refresh, 3000)
    let off = () => {}
    void onExport((e) => {
      if (sends.ownsExport(e.id)) return
      exp = e.state === 'run' ? { ...e, label: exp?.label } : null
      if (e.state !== 'run') pump()
      if (e.state === 'done') { exported = e.file ?? ''; notify.info(e.mode === 'copy' ? `Exporté sans réencodage : ${e.file}` : `Exporté : ${e.file}`); void refresh() }
      else if (e.state === 'error') notify.info(`Export impossible : ${e.message}`)
    }).then((f) => (off = f))
    return () => { clearInterval(t); off() }
  })

  $effect(() => {
    for (const r of list.slice(0, shown)) {
      const key = `${r.name}|${r.size}|${r.modified}`
      if (!(key in thumbs)) {
        thumbs[key] = ''
        replaysThumb(r.name).then((u) => (thumbs[key] = u), () => {})
      }
    }
  })

  async function act(fn: () => Promise<unknown>) {
    try { await fn(); await refresh() } catch (e) { notify.info(messageOf(e)) }
  }
  async function rename(r: Replay) {
    const to = await saisir({ titre: 'Renommer le replay', action: 'Renommer', saisie: { label: 'Nouveau nom', valeur: r.name.replace(/\.mp4$/i, '') } })
    if (to && to !== r.name.replace(/\.mp4$/i, '')) void act(() => replaysRename(r.name, to))
  }
  async function remove(r: Replay) {
    if (await confirmer({ titre: 'Supprimer le replay', message: `Supprimer « ${r.name} » ? Le fichier sera effacé du disque.`, action: 'Supprimer le replay', destructif: true })) void act(() => replaysDelete(r.name))
  }
  const toggleSelected = (name: string) => (selected = selected.includes(name) ? selected.filter((n) => n !== name) : [...selected, name])
  async function removeSelected() {
    const names = [...selected]
    if (!names.length || !(await confirmer({ titre: 'Supprimer les replays', message: `Supprimer ${names.length} replay(s) ? Les fichiers seront effacés du disque.`, action: 'Supprimer les replays', destructif: true }))) return
    void act(async () => {
      const failed: string[] = []
      for (const n of names) await replaysDelete(n).catch(() => failed.push(n))
      selected = []
      if (failed.length) notify.info(`Suppression impossible : ${failed.join(', ')}`)
    })
  }

  // --- Lecteur et édition ---
  let video = $state<HTMLVideoElement>()
  let hoverVideo = $state<HTMLVideoElement>()
  let stage = $state<HTMLDivElement>()
  let bar = $state<HTMLDivElement>()
  let src = $state('')
  let time = $state(0)
  let paused = $state(true)
  let speed = $state(1)
  let volume = $state(1)
  let start = $state(0)
  let end = $state(0)
  let crop = $state<Crop | null>(null)
  let ratio = $state(0)
  let quality = $state<Quality>('original')
  let hover = $state<{ x: number; t: number } | null>(null)
  let exp = $state<{ id: number; pct: number; label?: string } | null>(null)
  let markers = $state<Marker[]>([])
  let before = $state(10)
  let after = $state(5)
  let tracks = $state<AudioTrack[]>([])
  let trackSrc = $state<string[]>([])
  let mixed = $state(true)
  let keyframes = $state<number[]>([])
  let encoders = $state<string[]>([])
  let encoder = $state('auto')
  let plan = $state<ExportPlan | null>(null)
  let queue = $state<{ params: ExportParams; label: string }[]>([])
  const frame = $derived(frameDuration(current?.fps ?? 0))
  let audios: (HTMLAudioElement | undefined)[] = []
  // Le lecteur du webview ne lit qu'une piste : les pistes sont jouées à part, calées sur la vidéo, qui est alors muette.
  const live = $derived(tracks.length > 0 && trackSrc.length > 0)

  async function open(r: Replay) {
    current = r
    markers = r.markers ?? []
    src = await replayUrl(lib!.dir, r.name)
    start = 0
    end = r.duration
    crop = null
    ratio = 0
    time = 0
    tracks = audioTracks(r)
    trackSrc = []
    mixed = true
    keyframes = []
    replaysKeyframes(r.name).then((k) => { if (current === r) keyframes = k }, () => {})
    if (tracks.length) replaysTracks(r.name).then((p) => { if (current === r) trackSrc = p }, () => notify.info('Pistes audio illisibles : le son du fichier est joué tel quel.'))
  }
  const syncAudio = () => { if (video) for (const a of audios) if (a) a.currentTime = video.currentTime }
  $effect(() => { tracks.forEach((t, i) => { const a = audios[i]; if (a) a.volume = trackVolume(t, volume) }) })
  $effect(() => { for (const a of audios) if (a) a.playbackRate = speed })
  $effect(() => {
    if (!live) return
    for (const a of audios) if (a) { if (paused) a.pause(); else { a.currentTime = video?.currentTime ?? 0; void a.play() } }
  })
  /** Un seul déplacement à la fois : pendant un glisser, seule la dernière position demandée est jouée une fois le précédent terminé. */
  function seeker(el: () => HTMLVideoElement | undefined) {
    let pending: number | null = null
    const flush = () => { const v = el(); if (v && pending !== null && !v.seeking) { v.currentTime = pending; pending = null } }
    return { to: (t: number) => { pending = t; flush() }, done: flush }
  }
  const mainSeek = seeker(() => video)
  const hoverSeek = seeker(() => hoverVideo)
  let scrubbing = false
  const seek = (t: number) => {
    const v = Math.min(Math.max(0, t), current?.duration ?? t)
    time = v
    mainSeek.to(v)
  }
  const fullscreen = () => (document.fullscreenElement ? void document.exitFullscreen() : void stage?.requestFullscreen())
  /** Tête de lecture fluide : lue à chaque image tant que la vidéo joue (`timeupdate` n'arrive que 4 fois par seconde). */
  $effect(() => {
    if (paused || !video) return
    let id = 0
    const tick = () => {
      if (!scrubbing && video) time = video.currentTime
      if (video && live && audios.some((a) => a && Math.abs(a.currentTime - video!.currentTime) > 0.12)) syncAudio()
      id = requestAnimationFrame(tick)
    }
    id = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(id)
  })
  const toggle = () => (video?.paused ? void video.play() : video?.pause())
  const frac = (t: number) => (current?.duration ? (t / current.duration) * 100 : 0)

  function timeAt(e: PointerEvent) {
    const b = bar!.getBoundingClientRect()
    return Math.min(1, Math.max(0, (e.clientX - b.left) / b.width)) * current!.duration
  }
  /** Glisse une poignée de découpe (`start`/`end`) ou la tête de lecture. */
  function drag(e: PointerEvent, what: 'head' | 'start' | 'end') {
    e.preventDefault()
    e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    scrubbing = true
    const move = (ev: PointerEvent) => {
      const t = timeAt(ev)
      const snap = (x: number) => (ev.altKey ? x : snapToKeyframe(x, keyframes, (8 / bar!.getBoundingClientRect().width) * current!.duration))
      if (what === 'start') { start = Math.min(snap(t), end - 0.1); seek(start) }
      else if (what === 'end') { end = Math.max(snap(t), start + 0.1); seek(end) }
      else seek(t)
    }
    const up = () => { scrubbing = false; removeEventListener('pointermove', move); removeEventListener('pointerup', up) }
    addEventListener('pointermove', move)
    addEventListener('pointerup', up)
    move(e)
  }
  function barHover(e: PointerEvent) {
    const t = timeAt(e)
    hover = { x: e.clientX - bar!.getBoundingClientRect().left, t }
    hoverSeek.to(t)
  }

  function keys(e: KeyboardEvent) {
    if (!current || !video || (e.target as HTMLElement).closest('input, select, textarea')) return
    const k = e.key.toLowerCase()
    const acts: Record<string, () => void> = {
      ' ': toggle, k: toggle,
      arrowleft: () => seek(video!.currentTime - 5), arrowright: () => seek(video!.currentTime + 5),
      j: () => seek(video!.currentTime - 10), l: () => seek(video!.currentTime + 10),
      ',': () => { video!.pause(); seek(video!.currentTime - frame) }, '.': () => { video!.pause(); seek(video!.currentTime + frame) },
      arrowup: () => (volume = Math.min(1, volume + 0.1)), arrowdown: () => (volume = Math.max(0, volume - 0.1)),
      f: fullscreen, m: () => void addMarker(),
    }
    if (acts[k]) { e.preventDefault(); acts[k]() }
  }

  // --- Recadrage : on trace une zone sur l'image, ou on déplace celle qui existe ---
  function cropDrag(e: PointerEvent) {
    if (!current) return
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const [W, H] = [current.width, current.height]
    const pt = (ev: PointerEvent) => [Math.min(W, Math.max(0, ((ev.clientX - box.left) / box.width) * W)), Math.min(H, Math.max(0, ((ev.clientY - box.top) / box.height) * H))]
    const [x0, y0] = pt(e)
    const moving = crop && x0 >= crop.x && x0 <= crop.x + crop.w && y0 >= crop.y && y0 <= crop.y + crop.h ? { ...crop } : null
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    const el = e.currentTarget as HTMLElement
    const move = (ev: PointerEvent) => {
      const [x, y] = pt(ev)
      if (moving) {
        crop = { ...moving, x: Math.min(W - moving.w, Math.max(0, Math.round(moving.x + x - x0))), y: Math.min(H - moving.h, Math.max(0, Math.round(moving.y + y - y0))) }
        return
      }
      let w = Math.abs(x - x0), h = Math.abs(y - y0)
      if (ratio) { h = w / ratio; if (h > (y >= y0 ? H - y0 : y0)) { h = y >= y0 ? H - y0 : y0; w = h * ratio } }
      crop = { x: Math.round(x >= x0 ? x0 : x0 - w), y: Math.round(y >= y0 ? y0 : y0 - h), w: Math.round(w), h: Math.round(h) }
    }
    const up = () => { el.removeEventListener('pointermove', move); el.removeEventListener('pointerup', up); if (crop && (crop.w < 32 || crop.h < 32)) crop = null }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', up)
  }
  function cornerDrag(e: PointerEvent, corner: 'nw' | 'ne' | 'sw' | 'se') {
    if (!current || !crop) return
    e.stopPropagation()
    const layer = (e.currentTarget as HTMLElement).closest('.rp-crop-layer')!.getBoundingClientRect()
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    const move = (ev: PointerEvent) => (crop = resizeCrop(crop!, corner, ((ev.clientX - layer.left) / layer.width) * current!.width, ((ev.clientY - layer.top) / layer.height) * current!.height, current!.width, current!.height))
    const up = () => { el.removeEventListener('pointermove', move); el.removeEventListener('pointerup', up) }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', up)
  }
  const setCrop = (patch: Partial<Crop>) => current && crop && (crop = clampCrop({ ...crop, ...patch }, current.width, current.height))
  const num = (e: Event) => Math.round(Number((e.currentTarget as HTMLInputElement).value) || 0)
  function setTime(which: 'start' | 'end', e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const t = parseTimecode(input.value)
    const d = current!.duration
    if (t !== null) {
      if (which === 'start') start = Math.min(Math.max(0, t), end - 0.1)
      else end = Math.min(Math.max(t, start + 0.1), d)
      seek(which === 'start' ? start : end)
    }
    input.value = formatTimecode(which === 'start' ? start : end)
  }
  const pickRatio = () => current && (crop = cropForRatio(ratio || null, current.width, current.height) ?? crop)

  /** Graduations de la règle : environ 8 sur la durée, à un pas rond. */
  const ticks = $derived.by(() => {
    const d = current?.duration ?? 0
    const step = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600].find((n) => d / n <= 8) ?? 900
    return Array.from({ length: Math.floor(d / step) + 1 }, (_, i) => i * step)
  })
  async function setMarkers(next: Marker[]) {
    if (!current) return
    try {
      await replaysMarkersSet(current.name, next)
      markers = next
      void refresh()
    } catch (e) {
      notify.info(messageOf(e))
    }
  }
  const addMarker = () => setMarkers(withMarker(markers, { t: time, name: markerName(markers) }))
  async function renameMarker(m: Marker) {
    const to = await saisir({ titre: 'Renommer le marqueur', action: 'Renommer', saisie: { label: 'Nom du marqueur', valeur: m.name } })
    if (to !== null) void setMarkers(withMarker(markers, m, to))
  }
  const removeMarker = (m: Marker) => setMarkers(withMarker(markers, m, null))
  /** Découpe rapide : la plage autour du marqueur, exportée tout de suite. */
  function cutAround(m: Marker) {
    ;[start, end] = clipAround(m.t, before, after, current!.duration)
    seek(m.t)
    void doExport()
  }
  const estimate = $derived(current ? estimateSize(current, start, end, crop, quality) : 0)
  const request = (): ExportParams => ({ name: current!.name, start, end, crop: crop && $state.snapshot(crop), quality, audio: audioMix($state.snapshot(tracks), mixed), encoder })
  /** Indique avant l'export si la découpe sera copiée ou réencodée (relu après chaque réglage). */
  $effect(() => {
    if (!current) return
    const params = request()
    let stale = false
    const t = setTimeout(() => replaysPlan(params).then((p) => { if (!stale) plan = p }, () => { if (!stale) plan = null }), 250)
    return () => { stale = true; clearTimeout(t) }
  })
  /** Les exports se font un par un : celui-ci part tout de suite s'il n'y en a pas en cours, sinon il attend son tour. */
  function doExport() {
    if (!current) return
    queue.push({ params: request(), label: `${current.name} · ${formatDuration(end - start)}` })
    pump()
  }
  let starting = false
  async function pump() {
    if (exp || starting || !queue.length) return
    starting = true
    const job = queue.shift()!
    try {
      const id = await replaysExport($state.snapshot(job.params))
      exp = { id, pct: 0, label: job.label }
    } catch (e) {
      notify.info(messageOf(e))
      queueMicrotask(pump)
    } finally {
      starting = false
    }
  }
</script>

<svelte:window onkeydown={keys} onfocus={refresh} />

{#if error}<p class="small" style="color: var(--danger)">{error}</p>{/if}
{#if sending && lib}<ReplaySend replay={sending} dir={lib.dir} onclose={() => (sending = null)} />{/if}

<div class="replays">
{#if current}
  <div class="rp-editor">
    <div class="row rp-top">
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => { video?.pause(); current = null }}>← Bibliothèque</button>
      <strong class="truncate">{current.name}</strong>
      <span class="muted small">{current.width}×{current.height}</span>
      <button class="plbls-button plbls-button--compact" onclick={() => (sending = current)}>Envoyer dans…</button>
    </div>

    <div class="rp-stage" bind:this={stage}>
      <div class="rp-viewport">
        <div class="rp-frame" style="aspect-ratio: {current.width || 16} / {current.height || 9}; width: min(100cqw, calc(100cqh * {current.width || 16} / {current.height || 9}))">
          <!-- svelte-ignore a11y_media_has_caption -->
          <video bind:this={video} {src} bind:paused bind:volume muted={live} ontimeupdate={() => { if (!scrubbing) time = video!.currentTime }} onseeked={() => { mainSeek.done(); syncAudio() }} onclick={toggle}
            onloadedmetadata={() => { video!.playbackRate = speed }}></video>
          {#each tracks as t, i (t.index)}<audio bind:this={audios[i]} src={trackSrc[t.index]} preload="auto"></audio>{/each}
          <div class="rp-crop-layer" role="presentation" onpointerdown={cropDrag}>
            {#if crop}
              <div class="rp-crop" style="left:{(crop.x / current.width) * 100}%; top:{(crop.y / current.height) * 100}%; width:{(crop.w / current.width) * 100}%; height:{(crop.h / current.height) * 100}%">
                {#each ['nw', 'ne', 'sw', 'se'] as const as c (c)}
                  <div class="rp-corner rp-corner--{c}" role="presentation" onpointerdown={(e) => cornerDrag(e, c)}></div>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      </div>

      <div class="rp-timeline">
        <div class="rp-ruler" aria-hidden="true">
          {#each ticks as t (t)}<span style="left:{frac(t)}%">{formatDuration(t)}</span>{/each}
        </div>
        <div class="rp-bar" bind:this={bar} role="slider" tabindex="0" aria-label="Position" aria-valuemin="0" aria-valuemax={current.duration} aria-valuenow={time}
          onpointerdown={(e) => drag(e, 'head')} onpointermove={barHover} onpointerleave={() => (hover = null)}>
          <div class="rp-outside" style="left:0; width:{frac(start)}%"></div>
          <div class="rp-outside" style="left:{frac(end)}%; right:0"></div>
          <div class="rp-cut" style="left:{frac(start)}%; width:{frac(end - start)}%"></div>
          <div class="rp-handle rp-handle--start" role="presentation" style="left:{frac(start)}%" title="Début de la découpe" onpointerdown={(e) => drag(e, 'start')}></div>
          <div class="rp-handle rp-handle--end" role="presentation" style="left:{frac(end)}%" title="Fin de la découpe" onpointerdown={(e) => drag(e, 'end')}></div>
          {#each markers as m (m.t)}
            <button class="rp-mark" style="left:{frac(m.t)}%" title="{m.name} · {formatDuration(m.t)}" aria-label="Aller au marqueur {m.name}" onpointerdown={(e) => e.stopPropagation()} onclick={() => seek(m.t)}></button>
          {/each}
          <div class="rp-playhead" role="presentation" style="left:{frac(time)}%"></div>
          {#if hover}
            <div class="rp-preview" style="left:{hover.x}px">
              <!-- svelte-ignore a11y_media_has_caption -->
              <video bind:this={hoverVideo} {src} muted preload="auto" onseeked={hoverSeek.done}></video>
              <span>{formatDuration(hover.t)}</span>
            </div>
          {/if}
        </div>
      </div>

      <div class="row rp-transport">
        <button class="rp-tbtn" aria-label="Début de la vidéo" title="Début (Origine)" onclick={() => seek(0)}>⏮</button>
        <button class="rp-tbtn" aria-label="Reculer de 10 secondes" title="−10 s (J)" onclick={() => seek(time - 10)}>−10 s</button>
        <button class="rp-tbtn" aria-label="Image précédente" title="Image précédente (,)" onclick={() => { video?.pause(); seek(time - frame) }}>◂|</button>
        <button class="rp-tbtn rp-tbtn--main" aria-label={paused ? 'Lecture' : 'Pause'} title="Lecture / pause (espace)" onclick={toggle}>{paused ? '▶' : '❚❚'}</button>
        <button class="rp-tbtn" aria-label="Image suivante" title="Image suivante (.)" onclick={() => { video?.pause(); seek(time + frame) }}>|▸</button>
        <button class="rp-tbtn" aria-label="Avancer de 10 secondes" title="+10 s (L)" onclick={() => seek(time + 10)}>+10 s</button>
        <button class="rp-tbtn" aria-label="Fin de la vidéo" title="Fin" onclick={() => seek(current!.duration)}>⏭</button>
        <span class="rp-time">{formatDuration(time)} <span class="muted">/ {formatDuration(current.duration)}</span></span>
        <span class="rp-spacer"></span>
        <select class="plbls-select" aria-label="Vitesse" bind:value={speed} onchange={() => video && (video.playbackRate = speed)}>
          {#each SPEEDS as s (s)}<option value={s}>{s}×</option>{/each}
        </select>
        <button class="rp-tbtn" aria-label={volume ? 'Couper le son' : 'Rétablir le son'} onclick={() => (volume = volume ? 0 : 1)}>{volume ? '🔊' : '🔇'}</button>
        <input type="range" class="rp-vol" min="0" max="1" step="0.05" aria-label="Volume" bind:value={volume} />
        <button class="rp-tbtn" aria-label="Plein écran" title="Plein écran (F)" onclick={fullscreen}>⛶</button>
      </div>
    </div>

    <div class="rp-tools">
      <div class="rp-tool">
        <strong>Découpe</strong>
        <div class="row">
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (start = Math.min(time, end - 0.1))}>Début ici</button>
          <input class="plbls-input rp-tc" aria-label="Début de la découpe (m:ss.d)" value={formatTimecode(start)} onchange={(e) => setTime('start', e)} />
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (end = Math.max(time, start + 0.1))}>Fin ici</button>
          <input class="plbls-input rp-tc" aria-label="Fin de la découpe (m:ss.d)" value={formatTimecode(end)} onchange={(e) => setTime('end', e)} />
          <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => { start = 0; end = current!.duration }}>Tout garder</button>
          <span class="muted small">Durée : {formatDuration(end - start)}</span>
        </div>
        <span class="muted small">Les poignées s'aimantent sur les images clés (Alt : sans aimantation).</span>
      </div>
      <div class="rp-tool">
        <strong>Recadrage</strong>
        <div class="row">
          <select class="plbls-select" aria-label="Ratio" bind:value={ratio} onchange={pickRatio}>
            {#each RATIOS as [label, v] (label)}<option value={v}>Ratio : {label}</option>{/each}
          </select>
          <button class="plbls-button plbls-button--compact plbls-button--discret" disabled={!crop} onclick={() => (crop = null)}>Retirer</button>
          {#if !crop}<span class="muted small">Tracez une zone sur l’image</span>{/if}
        </div>
        {#if crop}
          <div class="row">
            {#each [['x', 'X'], ['y', 'Y'], ['w', 'Largeur'], ['h', 'Hauteur']] as const as [k, label] (k)}
              <label class="small">{label} <input class="plbls-input rp-num" type="number" min="0" step="2" aria-label={label} value={crop[k]} onchange={(e) => setCrop({ [k]: num(e) })} /></label>
            {/each}
          </div>
        {/if}
      </div>
      <div class="rp-tool rp-markers">
        <strong>Marqueurs</strong>
        <div class="row">
          <button class="plbls-button plbls-button--compact plbls-button--contour" title="Ajouter un marqueur (M)" onclick={addMarker}>Ajouter ici</button>
          <label class="small">Extrait : <input class="plbls-input rp-secs" type="number" min="0" max="600" aria-label="Secondes avant" bind:value={before} /> s avant,
            <input class="plbls-input rp-secs" type="number" min="0" max="600" aria-label="Secondes après" bind:value={after} /> s après</label>
        </div>
        {#if !markers.length}<span class="muted small">Aucun marqueur. Pose-en pendant le jeu avec le raccourci de Bobyn.</span>{/if}
        <ul class="rp-marklist">
          {#each markers as m (m.t)}
            <li class="row">
              <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => seek(m.t)}>{formatDuration(m.t)}</button>
              <span class="truncate" title={m.name}>{m.name}</span>
              <button class="plbls-button plbls-button--compact plbls-button--discret" disabled={!!exp} onclick={() => cutAround(m)}>Extrait</button>
              <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => renameMarker(m)}>Renommer</button>
              <button class="plbls-button plbls-button--compact plbls-button--discret" aria-label="Supprimer {m.name}" onclick={() => removeMarker(m)}>✕</button>
            </li>
          {/each}
        </ul>
      </div>
      {#if tracks.length}
        <div class="rp-tool">
          <strong>Pistes audio</strong>
          {#each tracks as t (t.index)}
            <div class="row">
              <label class="plbls-checkbox">
                <input type="checkbox" class="plbls-checkbox__input" bind:checked={t.on} />
                <span class="plbls-checkbox__case" aria-hidden="true"></span>
                <span class="plbls-checkbox__texte truncate">{t.name}</span>
              </label>
              <input type="range" min="0" max="1" step="0.05" bind:value={t.volume} aria-label="Volume de {t.name}" />
            </div>
          {/each}
          <select class="plbls-select" aria-label="Pistes à l'export" bind:value={mixed}>
            <option value={true}>Export : une piste mixée</option>
            <option value={false}>Export : pistes séparées</option>
          </select>
        </div>
      {/if}
      <div class="rp-tool">
        <strong>Export</strong>
        <div class="row">
          <select class="plbls-select" aria-label="Qualité" bind:value={quality}>
            <option value="original">Qualité originale</option>
            <option value="1080">1080p</option>
            <option value="720">720p</option>
          </select>
          <select class="plbls-select" aria-label="Encodeur" bind:value={encoder} disabled={plan?.copy}>
            <option value="auto">Encodeur : automatique</option>
            {#each encoders as enc (enc)}<option value={enc}>{enc.replace('h264_', '')}</option>{/each}
          </select>
          <span class="small">≈ {formatSize(estimate)}</span>
          <button class="plbls-button" onclick={doExport}>{exp ? 'Ajouter à la file' : 'Exporter un nouveau fichier'}</button>
        </div>
        {#if plan}<span class="small" class:muted={plan.copy}>{planLabel(plan)}</span>{/if}
        {#if exp || queue.length}
          <ul class="rp-marklist">
            {#if exp}
              <li class="row">
                <span class="truncate" title={exp.label}>{exp.label ?? 'Export en cours'}</span>
                <progress max="1" value={exp.pct} aria-label="Progression de l'export"></progress>
                <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => exp && void replaysCancel(exp.id)}>Annuler</button>
              </li>
            {/if}
            {#each queue as job, i (job)}
              <li class="row">
                <span class="truncate muted" title={job.label}>En attente : {job.label}</span>
                <button class="plbls-button plbls-button--compact plbls-button--discret" aria-label="Retirer de la file" onclick={() => queue.splice(i, 1)}>✕</button>
              </li>
            {/each}
          </ul>
        {/if}
        <div class="row">
          {#if lib?.replays.find((r) => r.name === exported)}
            <button class="plbls-button plbls-button--contour" onclick={() => (sending = lib!.replays.find((r) => r.name === exported)!)}>Envoyer « {exported} » dans…</button>
          {/if}
        </div>
      </div>
    </div>
    <p class="muted small rp-keys">Espace/K lecture · ←/→ 5 s · J/L 10 s · , et . image par image · ↑/↓ volume · F plein écran · M marqueur. L’original n’est pas modifié ; une découpe seule qui commence sur une image clé n’est pas réencodée.</p>
  </div>
{:else if lib}
  <div class="row">
    <input class="plbls-input" type="search" placeholder="Rechercher un replay" aria-label="Rechercher" bind:value={query} />
    <select class="plbls-select" aria-label="Jeu" bind:value={game}>
      <option value="">Tous les jeux</option>
      {#each games as g (g)}<option value={g}>{g}</option>{/each}
    </select>
    <select class="plbls-select" aria-label="Tri" bind:value={sort}>
      <option value="date">Plus récents</option>
      <option value="duration">Plus longs</option>
      <option value="size">Plus lourds</option>
    </select>
  </div>
  <p class="muted small">
    {lib.replays.length} replay(s) · {formatSize(used)} · <span class="truncate">{lib.dir}</span>
    <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => replaysReveal(null)}>Ouvrir le dossier</button>
    {#if selected.length}
      <button class="plbls-button plbls-button--compact" onclick={removeSelected}>Supprimer la sélection ({selected.length})</button>
      <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => (selected = [])}>Désélectionner</button>
    {/if}
  </p>
  {#if !list.length}<p class="muted small">Aucun replay.</p>{/if}
  <div class="rp-grid">
    {#each list.slice(0, shown) as r (r.name)}
      {@const thumb = thumbs[`${r.name}|${r.size}|${r.modified}`]}
      <div class="rp-card">
        <button class="rp-thumb" aria-label="Lire {r.name}" onclick={() => open(r)}>
          {#if thumb}<img src={thumb} alt="" loading="lazy" />{/if}
          <span class="rp-badge">{formatDuration(r.duration)}</span>
          {#if markerCount(r)}<span class="rp-badge rp-badge--marks">{markerCount(r)}</span>{/if}
        </button>
        <label class="plbls-checkbox">
          <input type="checkbox" class="plbls-checkbox__input" checked={selected.includes(r.name)} onchange={() => toggleSelected(r.name)} />
          <span class="plbls-checkbox__case" aria-hidden="true"></span>
          <span class="plbls-checkbox__texte truncate" title={r.name}>{r.name}</span>
        </label>
        <span class="muted small">{r.game || '—'} · {new Date(r.modified * 1000).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' })} · {r.width}×{r.height} · {formatSize(r.size)}</span>
        {#if audioTracks(r).length}<span class="muted small" title="Pistes audio">🔊 {audioTracks(r).map((t) => t.name).join(' · ')}</span>{/if}
        <div class="row">
          <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => rename(r)}>Renommer</button>
          <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => replaysReveal(r.name)}>Dossier</button>
          <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => act(() => replaysCopy(r.name)).then(() => notify.info('Fichier copié : collez-le où vous voulez.'))}>Copier</button>
          <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => remove(r)}>Supprimer</button>
        </div>
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (sending = r)}>Envoyer dans…</button>
      </div>
    {/each}
  </div>
  {#if list.length > shown}
    <button class="plbls-button plbls-button--contour" onclick={() => (shown += PAGE)}>Afficher plus ({list.length - shown})</button>
  {/if}
{:else if !error}
  <p class="muted small">Chargement…</p>
{/if}
</div>

<style>
  .replays { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 0.75rem; }
  .replays > :global(*) { flex-shrink: 0; }
  .replays > .rp-editor { flex-shrink: 1; }
  .rp-editor { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 0.75rem; }
  .rp-editor > :global(*) { flex-shrink: 0; }
  .rp-editor > .rp-stage { flex: 1 1 auto; min-height: 280px; }
  .rp-top { gap: 0.75rem; }
  .rp-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 12px; }
  .rp-card { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .rp-thumb { position: relative; aspect-ratio: 16 / 9; padding: 0; border: 0; border-radius: 8px; overflow: hidden; background: var(--surface-2, #0002); cursor: pointer; }
  .rp-thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .rp-badge--marks { right: auto; left: 6px; background: #1d4ed8cc; }
  .rp-badge { position: absolute; right: 6px; bottom: 6px; padding: 1px 6px; border-radius: 4px; background: #000a; color: #fff; font-size: 12px; }

  .rp-stage { display: flex; flex-direction: column; gap: 0.5rem; min-height: 0; background: #0b0b0d; border-radius: 10px; padding: 0.5rem; }
  .rp-stage > :global(*) { flex-shrink: 0; }
  .rp-viewport { flex: 1 1 0; min-height: 0; container-type: size; display: grid; place-items: center; }
  .rp-frame { position: relative; background: #000; }
  .rp-frame video { width: 100%; height: 100%; display: block; }
  .rp-crop-layer { position: absolute; inset: 0; cursor: crosshair; touch-action: none; }
  .rp-crop { position: absolute; border: 2px solid #fff; box-shadow: 0 0 0 9999px #0008; pointer-events: none; }

  .rp-timeline { padding: 0 6px; }
  .rp-ruler { position: relative; height: 16px; font-size: 11px; color: #aaa; }
  .rp-ruler span { position: absolute; transform: translateX(-50%); white-space: nowrap; }
  .rp-ruler span:first-child { transform: none; }
  .rp-bar { position: relative; height: 56px; background: #2a2a30; border-radius: 6px; cursor: pointer; touch-action: none; user-select: none; }
  .rp-cut { position: absolute; top: 0; bottom: 0; background: color-mix(in srgb, var(--accent, #4a8) 30%, transparent); border-block: 2px solid var(--accent, #4a8); }
  .rp-outside { position: absolute; top: 0; bottom: 0; background: #0009; border-radius: 6px; pointer-events: none; }
  .rp-handle { position: absolute; top: -3px; bottom: -3px; width: 12px; background: var(--accent, #4a8); cursor: ew-resize; }
  .rp-handle--start { margin-left: -12px; border-radius: 6px 0 0 6px; }
  .rp-handle--end { border-radius: 0 6px 6px 0; }
  .rp-mark { position: absolute; top: 0; bottom: 0; width: 10px; margin-left: -5px; padding: 0; border: 0; background: transparent; cursor: pointer; z-index: 1; }
  .rp-mark::before { content: ''; position: absolute; left: 4px; top: 0; bottom: 0; width: 2px; background: #60a5fa; }
  .rp-mark::after { content: ''; position: absolute; left: 0; top: 0; border: 5px solid transparent; border-top-color: #60a5fa; }
  .rp-playhead { position: absolute; top: -8px; bottom: -4px; width: 2px; margin-left: -1px; background: #fff; pointer-events: none; }
  .rp-playhead::before { content: ''; position: absolute; top: 0; left: -5px; border: 6px solid transparent; border-top-color: #fff; }
  .rp-preview { position: absolute; bottom: 64px; transform: translateX(-50%); display: flex; flex-direction: column; align-items: center; pointer-events: none; z-index: 2; }
  .rp-preview video { width: 160px; border-radius: 4px; background: #000; }
  .rp-preview span { font-size: 12px; background: #000a; color: #fff; padding: 0 4px; border-radius: 3px; }

  .rp-transport { flex-wrap: wrap; gap: 0.35rem; align-items: center; }
  .rp-spacer { flex: 1; }
  .rp-time { font-variant-numeric: tabular-nums; margin: 0 0.5rem; }
  .rp-tbtn { min-width: 36px; height: 32px; padding: 0 8px; border: 0; border-radius: 6px; background: #2a2a30; color: #eee; cursor: pointer; }
  .rp-tbtn:hover { background: #3a3a42; }
  .rp-tbtn--main { min-width: 48px; background: var(--accent, #4a8); color: #000; }
  .rp-vol { width: 90px; }
  .rp-tools { display: flex; flex-wrap: wrap; gap: 1rem 2rem; }
  .rp-tool { display: flex; flex-direction: column; gap: 0.35rem; }
  .rp-tool .row { flex-wrap: wrap; gap: 0.5rem; align-items: center; }
  progress { width: 140px; }
  .rp-tc { width: 6rem; }
  .rp-num { width: 5rem; }
  .rp-corner { position: absolute; width: 14px; height: 14px; background: #fff; border-radius: 3px; pointer-events: auto; touch-action: none; }
  .rp-corner--nw { left: -7px; top: -7px; cursor: nwse-resize; }
  .rp-corner--se { right: -7px; bottom: -7px; cursor: nwse-resize; }
  .rp-corner--ne { right: -7px; top: -7px; cursor: nesw-resize; }
  .rp-corner--sw { left: -7px; bottom: -7px; cursor: nesw-resize; }
  .rp-secs { width: 4.5rem; }
  .rp-marklist { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; max-width: 520px; }
  .rp-marklist .row { gap: 0.4rem; flex-wrap: nowrap; }
  .rp-keys { margin: 0; }
</style>
