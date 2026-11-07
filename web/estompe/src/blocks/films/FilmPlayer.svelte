<script lang="ts">
  import { onMount } from 'svelte'
  import { filmsApi } from '../../lib/api'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import WatchPicker from '../regarder-ensemble/WatchPicker.svelte'
  import { providers, type Adapter, type PState } from '../regarder-ensemble/providers'
  import { clock, fullTitle, resumeAt, trackName, type FilmDetail, type FilmTracks } from './films'

  let { id }: { id: string } = $props()

  let root = $state<HTMLElement>()
  let host = $state<HTMLElement>()
  let item = $state<FilmDetail | null>(null)
  let player = $state.raw<Adapter | null>(null)
  let st = $state<PState>('unstarted')
  let position = $state(0)
  let duration = $state(0)
  let drag = $state<number | null>(null)
  let volume = $state(100)
  let muted = $state(false)
  let tracks = $state<FilmTracks | null>(null)
  let panel = $state(false)
  let picker = $state(false)
  let error = $state('')

  const playing = $derived(st === 'playing' || st === 'buffering')
  const back = $derived(item?.series_id ? hrefs.films('item', item.series_id) : hrefs.films('item', id))
  const pct = (v: number, max: number) => `${max ? Math.min(100, (v / max) * 100) : 0}%`
  const toggle = () => (playing ? player?.pause() : player?.play())
  const seek = (s: number) => { player?.seek(Math.max(0, duration ? Math.min(s, duration) : s)); position = s }
  const fullscreen = () => { if (document.fullscreenElement) void document.exitFullscreen(); else void root?.requestFullscreen().catch(() => {}) }

  const toggleMute = () => { muted = !muted; if (!muted && volume === 0) volume = 100 }

  $effect(() => { const v = muted ? 0 : volume; player?.setVolume?.(v) })

  onMount(() => {
    let gone = false
    let poll: ReturnType<typeof setInterval> | undefined
    void (async () => {
      const it = await filmsApi.item(id)
      if (gone) return
      item = it
      const a = await providers.film(host!, {
        onState: (s) => { st = s; if (s === 'playing') error = '' },
        onError: (m) => { error = m },
        onTracks: (t) => { tracks = t },
      })
      if (gone) { a.destroy(); return }
      player = a
      a.load(id, resumeAt(it), true)
      poll = setInterval(() => { position = a.time(); duration = a.duration?.() || it.duration }, 500)
    })().catch((e) => { if (!gone) notify.error(e, 'La lecture n’a pas pu démarrer'); error = 'La lecture n’a pas pu démarrer.' })
    return () => { gone = true; clearInterval(poll); player?.destroy(); player = null }
  })
</script>

<div class="film-player" bind:this={root}>
  <div class="film-player-top">
    <a class="watch-btn" href={back} aria-label="Fermer le lecteur"><Icon name="x" size={22} /></a>
    <div class="grow film-player-title"><strong class="truncate">{item ? fullTitle(item) : ''}</strong><span class="film-player-sub">Regarder seul</span></div>
    {#if item}<button class="plbls-button v-rose film-to-voice" onclick={() => (picker = true)}><Icon name="film" size={18} /> Continuer au cinéma à {clock(position)}</button>{/if}
  </div>
  <div class="film-player-video">
    <div class="watch-video" bind:this={host}></div>
    {#if error}
      <div class="watch-msg plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>
    {:else if !playing && item}
      <button class="watch-big" onclick={toggle} aria-label="Lecture"><Icon name="lecture" size={34} /></button>
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
      </section>
    {/if}
  </div>
  <div class="film-player-bar" role="group" aria-label="Lecture">
    <input class="watch-range film-seek" style="--p:{pct(drag ?? position, duration)}" type="range" min="0" max={Math.max(duration, 1)} step="1" value={drag ?? position} aria-label="Position dans la vidéo"
      oninput={(e) => (drag = Number(e.currentTarget.value))}
      onchange={(e) => { seek(Number(e.currentTarget.value)); drag = null }} />
    <div class="film-player-buttons">
      <button class="watch-btn" onclick={toggle} aria-label={playing ? 'Pause' : 'Lecture'}><Icon name={playing ? 'pause' : 'lecture'} size={22} /></button>
      <button class="film-chip" onclick={() => seek(position - 10)} aria-label="Reculer de 10 secondes">−10 s</button>
      <button class="film-chip" onclick={() => seek(position + 30)} aria-label="Avancer de 30 secondes">+30 s</button>
      <span class="watch-time v-mono">{clock(drag ?? position)} / {clock(duration)}</span>
      <span class="grow"></span>
      <button class="watch-btn" onclick={toggleMute} aria-label={muted ? 'Rétablir le son' : 'Couper le son'} aria-pressed={muted}><Icon name={muted ? 'volcoupe' : 'volmax'} size={20} /></button>
      <input class="watch-range watch-vol" style="--p:{muted ? 0 : volume}%" type="range" min="0" max="100" value={muted ? 0 : volume} aria-label="Volume" oninput={(e) => { volume = Number(e.currentTarget.value); muted = volume === 0 }} />
      {#if tracks}<button class="film-chip" aria-expanded={panel} onclick={() => (panel = !panel)}><Icon name="bulle" size={16} /> Audio et sous-titres</button>{/if}
      <button class="watch-btn" onclick={fullscreen} aria-label="Plein écran"><Icon name="agrandir" size={20} /></button>
    </div>
  </div>
</div>
{#if picker}
  <WatchPicker links={[`film:${id}`]} kind="cinema" position={position} onclose={() => (picker = false)} />
{/if}
