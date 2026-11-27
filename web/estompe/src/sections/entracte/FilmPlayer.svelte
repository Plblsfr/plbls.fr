<script lang="ts">
  import { onMount } from 'svelte'
  import { filmsApi, requestAt } from '../../lib/api'
  import { hrefs, router } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { platform } from '../../lib/platform'
  import { notify } from '../../lib/ui'
  import { medias } from '../../lib/medias'
  import Icon from '../../components/Icon.svelte'
  import WatchPicker from '../../blocks/regarder-ensemble/WatchPicker.svelte'
  import { providers, type Adapter, type PState } from '../../blocks/regarder-ensemble/providers'
  import { AUDIO_PREFS, BUFFER_PREFS, bufferedAhead, clock, episodeAfter, IDLE_MS, fullTitle, inIntro, nearEnd, pickAudio, pickSubtitle, playPrefs, QUALITY_LABEL, resumeAt, savePlayPrefs, seasonAfter, seekTrack, SEEK_STEP, shortcut, SUBTITLE_PREFS, trackName, VOLUME_STEP, type AudioPref, type BufferPref, type CastState, type FilmDetail, type FilmItem, type FilmTracks, type Range, type SubtitlePref } from './films'

  let { id }: { id: string } = $props()

  let root = $state<HTMLElement>()
  let host = $state<HTMLElement>()
  let item = $state<FilmDetail | null>(null)
  let player = $state.raw<Adapter | null>(null)
  let st = $state<PState>('unstarted')
  let position = $state(0)
  let duration = $state(0)
  let buffered = $state.raw<Range[]>([])
  let drag = $state<number | null>(null)
  const store = () => { try { return localStorage } catch { return null } }
  const saved = playPrefs(store())
  let volume = $state(saved.volume)
  let muted = $state(saved.muted)
  let audioPref = $state<AudioPref>(saved.audio)
  let subtitlePref = $state<SubtitlePref>(saved.subtitles)
  let bufferPref = $state<BufferPref>(saved.buffer)
  let wifiOnly = $state(saved.wifiOnly)
  const reseauConnu = (platform.network?.kind() ?? 'unknown') !== 'unknown'
  let tracks = $state<FilmTracks | null>(null)
  let audioVoulu: number | null = null
  let panel = $state(false)
  let picker = $state(false)
  let error = $state('')
  let idle = $state(false)
  let idleTimer: ReturnType<typeof setTimeout> | undefined
  let cast = $state<CastState | null>(null)
  let next = $state<FilmItem | null>(null)

  const playing = $derived(st === 'playing' || st === 'buffering')
  const ahead = $derived(Math.floor(bufferedAhead(position, buffered)))
  const back = $derived(item?.series_id ? hrefs.films('item', item.series_id) : hrefs.films('item', id))
  const pct = (v: number, max: number) => `${max ? Math.min(100, (v / max) * 100) : 0}%`
  const toggle = () => (playing ? player?.pause() : player?.play())
  const seek = (s: number) => { player?.seek(Math.max(0, duration ? Math.min(s, duration) : s)); position = s }
  const intro = $derived(tracks?.intro ?? null)
  const skippable = $derived(!error && inIntro(position, intro))
  const skip = () => {
    if (!intro || !skippable) return
    seek(intro.fin)
    void requestAt<void>(filmsApi.base, 'POST', `/titles/${encodeURIComponent(id)}/intro-skipped`).catch(() => {})
  }
  const nextDue = $derived(!!next && !error && (st === 'ended' || nearEnd(position, duration)))
  const playNext = () => {
    if (!next) return
    void filmsApi.outroSkipped(id).catch(() => {})
    router.go(hrefs.films('watch', next.id))
  }
  const findNext = async (it: FilmDetail): Promise<FilmItem | null> => {
    if (it.kind !== 'episode' || !it.series_id || it.season == null) return null
    const seasons = (await filmsApi.seasons(it.series_id)).items
    const current = seasons.find((s) => s.season === it.season)
    const after = current ? episodeAfter((await filmsApi.episodes(current.id)).items, it.id) : undefined
    if (after) return after
    const season = seasonAfter(seasons, it.season)
    return (season && (await filmsApi.episodes(season.id)).items.find((e) => e.available !== false)) || null
  }
  const fullscreen = () => { if (document.fullscreenElement) void document.exitFullscreen(); else void root?.requestFullscreen().catch(() => {}) }

  const wake = () => {
    idle = false
    clearTimeout(idleTimer)
    idleTimer = setTimeout(() => { idle = playing && cast !== 'connected' && !panel && !picker && drag === null && !root?.querySelector(':focus-visible') }, IDLE_MS)
  }

  $effect(() => { void playing; void panel; void picker; void cast; wake() })

  const diffuser = async () => { const m = await player?.cast?.(); if (m) notify.fail(m) }

  const setSound = (v: number, m: boolean) => { volume = Math.min(100, Math.max(0, v)); muted = m; savePlayPrefs(store(), { volume, muted }) }
  const toggleMute = () => setSound(!muted || volume > 0 ? volume : 100, !muted)
  const nudge = (d: number) => { const v = Math.min(100, Math.max(0, (muted ? 0 : volume) + d)); setSound(v, v === 0) }

  const applySubtitle = (audioN: number) => {
    if (!tracks) return
    const subN = pickSubtitle(tracks.subtitles, subtitlePref, tracks.audio.find((a) => a.n === audioN))
    player?.setSubtitle?.(subN)
    tracks = { ...tracks, subN }
  }
  const preferAudio = (pref: AudioPref) => {
    audioPref = pref
    savePlayPrefs(store(), { audio: pref })
    if (!tracks) return
    const n = pickAudio(tracks.audio, pref)
    if (n === tracks.audioN) return
    if (subtitlePref === 'forced') applySubtitle(n)
    audioVoulu = n
    player?.setAudio?.(n)
  }
  const preferSubtitle = (pref: SubtitlePref) => {
    subtitlePref = pref
    savePlayPrefs(store(), { subtitles: pref })
    if (tracks) applySubtitle(audioVoulu ?? tracks.audioN)
  }

  const preferBuffer = (pref: BufferPref) => {
    bufferPref = pref
    savePlayPrefs(store(), { buffer: pref })
    player?.setBuffer?.(pref)
  }
  const preferWifi = (on: boolean) => {
    wifiOnly = on
    savePlayPrefs(store(), { wifiOnly: on })
    player?.setWifiOnly?.(on)
  }

  const ACTIONS = {
    toggle: () => toggle(),
    back: () => seek(position - SEEK_STEP),
    forward: () => seek(position + SEEK_STEP),
    louder: () => nudge(VOLUME_STEP),
    quieter: () => nudge(-VOLUME_STEP),
    mute: () => toggleMute(),
    fullscreen: () => fullscreen(),
    skip: () => skip(),
  }
  const key = (e: KeyboardEvent) => {
    const action = shortcut(e)
    if (!action || e.defaultPrevented || !player || picker) return
    if (action === 'skip' && !skippable) return
    if (document.querySelector('plbls-modal, [aria-modal="true"], [role="menu"]')) return
    const t = e.target instanceof HTMLElement ? e.target : null
    if (t?.isContentEditable || t?.matches('textarea, select, input:not([type="range"])')) return
    if ((e.key === ' ' || e.key.startsWith('Arrow')) && t?.matches('button, a, input') && t.matches(':focus-visible')) return
    e.preventDefault()
    if (e.key === ' ' && t?.matches('button, a')) t.blur()
    ACTIONS[action]()
    wake()
  }

  $effect(() => { const v = muted ? 0 : volume; player?.setVolume?.(v) })

  onMount(() => {
    let gone = false
    let poll: ReturnType<typeof setInterval> | undefined
    void (async () => {
      const a = await providers.film(host!, {
        onState: (s) => { if (s === 'playing' && st !== 'playing' && st !== 'buffering') medias.demarre('film'); st = s; if (s === 'playing') error = '' },
        onError: (m) => { error = m },
        onTracks: (t) => { tracks = t; audioVoulu = null },
        onCast: (s) => { cast = s },
      })
      if (gone) { a.destroy(); return }
      player = a
      poll = setInterval(() => { position = a.time(); duration = a.duration?.() || item?.duration || 0; buffered = a.buffered?.() ?? [] }, 500)
    })().catch((e) => { if (!gone) notify.error(e, 'La lecture n’a pas pu démarrer'); error = 'La lecture n’a pas pu démarrer.' })
    return () => { gone = true; clearInterval(poll); clearTimeout(idleTimer); player?.destroy(); player = null }
  })

  $effect(() => {
    const wanted = id
    const a = player
    if (!a) return
    let gone = false
    item = null
    tracks = null
    next = null
    error = ''
    st = 'unstarted'
    position = 0
    duration = 0
    buffered = []
    panel = false
    void (async () => {
      const it = await filmsApi.item(wanted)
      if (gone) return
      item = it
      a.load(wanted, resumeAt(it), true)
      const n = await findNext(it).catch(() => null)
      if (!gone) next = n
    })().catch((e) => { if (!gone) { notify.error(e, 'La lecture n’a pas pu démarrer'); error = 'La lecture n’a pas pu démarrer.' } })
    return () => { gone = true }
  })
</script>

<svelte:window onkeydown={key} />
<div class="film-player" class:idle bind:this={root} onpointermove={wake} onpointerdown={wake} onkeydown={wake} onfocusin={wake} role="presentation">
  <div class="film-player-top">
    <a class="watch-btn" href={back} aria-label="Fermer le lecteur"><Icon name="x" size={22} /></a>
    <div class="grow film-player-title"><strong class="truncate">{item ? fullTitle(item) : ''}</strong><span class="film-player-sub">Regarder seul</span></div>
    {#if item && app.phase === 'ready' && app.cinemaOn}<button class="plbls-button v-rose film-to-voice" onclick={() => (picker = true)}><Icon name="film" size={18} /> Continuer au cinéma à {clock(position)}</button>{/if}
  </div>
  <div class="film-player-video">
    <div class="watch-video" bind:this={host}></div>
    {#if error}
      <div class="watch-msg plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>
    {:else if !playing && item}
      <button class="watch-big" onclick={toggle} aria-label="Lecture"><Icon name="lecture" size={34} /></button>
    {:else if st === 'buffering'}
      <span class="plbls-spinner film-wait" role="status" aria-label="Chargement"></span>
    {/if}
    {#if cast === 'connected' && !error}
      <div class="watch-msg film-cast" role="status"><Icon name="diffuser" size={18} /> Diffusion sur un autre appareil</div>
    {/if}
    {#if skippable && !panel}
      <button class="plbls-button film-skip" onclick={skip} title="S" aria-keyshortcuts="S"><Icon name="suivant" size={16} /> Passer l’intro</button>
    {:else if nextDue && next && !panel}
      <button class="plbls-button film-skip" onclick={playNext} title={fullTitle(next)}><Icon name="suivant" size={16} /> Épisode suivant</button>
    {/if}
    {#if panel && tracks}
      <section class="film-tracks-panel" aria-label="Pistes">
        <div class="film-mh">Audio</div>
        {#each tracks.audio as a (a.n)}
          <button class="film-track" aria-pressed={a.n === tracks.audioN} onclick={() => player?.setAudio?.(a.n)}>{trackName(a)}</button>
        {/each}
        <div class="film-mh">Sous-titres</div>
        <button class="film-track" aria-pressed={tracks.subN === null} onclick={() => { player?.setSubtitle?.(null); tracks = { ...tracks!, subN: null } }}>Aucun</button>
        {#each tracks.subtitles as t (t.n)}
          <button class="film-track" aria-pressed={t.n === tracks.subN} onclick={() => { player?.setSubtitle?.(t.n); tracks = { ...tracks!, subN: t.n } }}>{trackName(t)}</button>
        {/each}
        {#if tracks.qualities && tracks.qualities.length > 1}
          <div class="film-mh">Qualité</div>
          {#each tracks.qualities as q (q)}
            <button class="film-track" aria-pressed={q === tracks.quality} onclick={() => player?.setQuality?.(q)}>{QUALITY_LABEL[q]}</button>
          {/each}
        {/if}
        <div class="film-mh">Par défaut, pour tous les titres</div>
        <div class="film-pref">
          <label for="film-pref-audio">Audio</label>
          <select id="film-pref-audio" value={audioPref} onchange={(e) => preferAudio(e.currentTarget.value as AudioPref)}>
            {#each AUDIO_PREFS as [v, nom] (v)}<option value={v}>{nom}</option>{/each}
          </select>
        </div>
        <div class="film-pref">
          <label for="film-pref-sous-titres">Sous-titres</label>
          <select id="film-pref-sous-titres" value={subtitlePref} onchange={(e) => preferSubtitle(e.currentTarget.value as SubtitlePref)}>
            {#each SUBTITLE_PREFS as [v, nom] (v)}<option value={v}>{nom}</option>{/each}
          </select>
        </div>
        <div class="film-pref">
          <label for="film-pref-prechargement">Préchargement</label>
          <select id="film-pref-prechargement" value={bufferPref} onchange={(e) => preferBuffer(e.currentTarget.value as BufferPref)}>
            {#each BUFFER_PREFS as [v, nom] (v)}<option value={v}>{nom}</option>{/each}
          </select>
        </div>
        {#if reseauConnu}
          <label class="film-pref-check"><input type="checkbox" checked={wifiOnly} onchange={(e) => preferWifi(e.currentTarget.checked)} /> Préchargement long en Wi-Fi seulement</label>
        {/if}
        <p class="film-pref-note">Garde la suite du film sur cet appareil jusqu'à la fermeture du titre. Sans effet quand le fichier est lu tel quel : le navigateur décide alors seul.</p>
      </section>
    {/if}
  </div>
  <div class="film-player-bar" role="group" aria-label="Lecture">
    <input class="watch-range film-seek" style="--p:{pct(drag ?? position, duration)};--piste:{seekTrack(drag ?? position, duration, buffered)}" type="range" min="0" max={Math.max(duration, 1)} step="1" value={drag ?? position} aria-label="Position dans la vidéo"
      oninput={(e) => (drag = Number(e.currentTarget.value))}
      onchange={(e) => { seek(Number(e.currentTarget.value)); drag = null }} />
    <div class="film-player-buttons">
      <button class="watch-btn" onclick={toggle} aria-label={playing ? 'Pause' : 'Lecture'} title="Espace ou K" aria-keyshortcuts="Space K"><Icon name={playing ? 'pause' : 'lecture'} size={22} /></button>
      <button class="film-chip" onclick={() => seek(position - 10)} aria-label="Reculer de 10 secondes" title="Flèche gauche ou J" aria-keyshortcuts="ArrowLeft J">−10 s</button>
      <button class="film-chip" onclick={() => seek(position + 30)} aria-label="Avancer de 30 secondes">+30 s</button>
      <span class="watch-time v-mono">{clock(drag ?? position)} / {clock(duration)}</span>
      {#if ahead > 0}<span class="film-ahead v-mono" title="Durée déjà chargée après la position en cours">{clock(ahead)} en avance</span>{/if}
      <span class="grow"></span>
      <button class="watch-btn" onclick={toggleMute} aria-label={muted ? 'Rétablir le son' : 'Couper le son'} aria-pressed={muted} title="M" aria-keyshortcuts="M"><Icon name={muted ? 'volcoupe' : 'volmax'} size={20} /></button>
      <input class="watch-range watch-vol" style="--p:{muted ? 0 : volume}%" type="range" min="0" max="100" value={muted ? 0 : volume} aria-label="Volume" title="Flèches haut et bas" oninput={(e) => { const v = Number(e.currentTarget.value); setSound(v, v === 0) }} />
      {#if tracks}<button class="film-chip" aria-expanded={panel} onclick={() => (panel = !panel)}><Icon name="bulle" size={16} /> {tracks.qualities && tracks.qualities.length > 1 ? 'Audio, sous-titres, qualité' : 'Audio et sous-titres'}</button>{/if}
      {#if cast && player?.cast}<button class="watch-btn" onclick={diffuser} aria-label={cast === 'disconnected' ? 'Diffuser sur une télé' : 'Diffusion en cours, changer d’appareil'} aria-pressed={cast !== 'disconnected'}><Icon name="diffuser" size={20} /></button>{/if}
      <button class="watch-btn" onclick={fullscreen} aria-label="Plein écran" title="F" aria-keyshortcuts="F"><Icon name="agrandir" size={20} /></button>
    </div>
  </div>
</div>
{#if picker}
  <WatchPicker links={[`film:${id}`]} kind="cinema" position={position} onclose={() => (picker = false)} />
{/if}
