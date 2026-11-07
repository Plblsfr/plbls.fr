<script lang="ts">
  import { ApiError, filmsApi } from '../../lib/api'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import WatchPicker from '../regarder-ensemble/WatchPicker.svelte'
  import FilmPoster from './FilmPoster.svelte'
  import { percent, pickEpisode, remaining, resumeAt, shortDuration, trackName, type FilmDetail, type FilmItem } from './films'

  let { id }: { id: string } = $props()

  let item = $state<FilmDetail | null>(null)
  let seasons = $state<{ season: FilmItem; episodes: FilmItem[] }[]>([])
  let shown = $state(0)
  let picker = $state(false)
  let failed = $state(false)

  const episodes = $derived(seasons[shown]?.episodes ?? [])
  const next = $derived(pickEpisode(seasons.flatMap((s) => s.episodes)))
  const play = $derived(item?.kind === 'series' ? next : item ?? undefined)
  const started = $derived(!!play && resumeAt(play) > 0)
  const prog = $derived(play?.progress && !play.progress.seen ? play.progress : null)
  const short = (e: FilmItem) => (e.season != null && e.episode != null ? `S${e.season}E${e.episode}` : '')
  const facts = $derived(item ? [item.kind === 'series' ? 'Série' : '', item.kind === 'series' && seasons.length ? `${seasons.length} saison${seasons.length > 1 ? 's' : ''}` : '', item.year, item.duration ? shortDuration(item.duration) : '', ...item.genres.slice(0, 3)].filter(Boolean).join(' · ') : '')

  $effect(() => {
    const wanted = id
    let gone = false
    item = null
    seasons = []
    failed = false
    void (async () => {
      const it = await filmsApi.item(wanted)
      const list = it.kind === 'series'
        ? await Promise.all((await filmsApi.seasons(wanted)).items.map(async (season) => ({ season, episodes: (await filmsApi.episodes(season.id)).items })))
        : []
      if (gone) return
      item = it
      seasons = list
      const cur = pickEpisode(list.flatMap((s) => s.episodes))
      shown = Math.max(0, list.findIndex((s) => s.episodes.some((e) => e.id === cur?.id)))
    })().catch((e) => { if (!gone) { failed = true; if (!(e instanceof ApiError && e.status === 404)) notify.error(e, 'Ce titre est indisponible') } })
    return () => { gone = true }
  })
</script>

{#if item}
  {@const title = item.title}
  <div class="film-page film-page--flush">
    <div class="film-hero">
      <span class="film-hero-art"><FilmPoster id={item.id} {title} image={item.backdrop} type="fond" width={1280} /></span>
      <a class="film-back plbls-button plbls-button--compact plbls-button--contour" href={hrefs.films()}><Icon name="gauche" size={14} /> Bibliothèque</a>
      <div class="film-hero-body">
        {#if item.kind !== 'series' || item.poster}<span class="film-card-art film-body-poster"><FilmPoster id={item.id} {title} image={item.poster} width={480} /></span>{/if}
        <div class="film-info">
          <p class="v-eyebrow">{facts}</p>
          <h1>{title}</h1>
          {#if item.overview}<p class="film-overview">{item.overview}</p>{/if}
          {#if play}
            {@const verb = `${started ? 'Reprendre' : 'Regarder'} seul`}
            <div class="film-actions">
              <a class="plbls-button" href={hrefs.films('watch', play.id)} aria-label={item.kind === 'series' && play.season != null && play.episode != null ? `${verb}, saison ${play.season} épisode ${play.episode}` : undefined}><Icon name="lecture" size={18} /> {verb}{item.kind === 'series' && short(play) ? ` · ${short(play)}` : ''}</a>
              <button class="plbls-button v-rose" onclick={() => (picker = true)}><Icon name="film" size={18} /> Regarder au cinéma</button>
              {#if prog}
                <span class="film-progress"><span class="v-mono v-mute">{remaining(prog)}</span><span class="film-bar"><span style="width:{percent(prog)}"></span></span></span>
              {:else if item.progress?.seen}
                <span class="v-tag ok film-seen"><Icon name="ok" size={14} /> Vu</span>
              {/if}
            </div>
          {/if}
          {#if item.audio.length || item.subtitles.length}
            <dl class="film-tracks v-note">
              {#if item.audio.length}<div><dt>Audio</dt><dd>{item.audio.map(trackName).join(' · ')}</dd></div>{/if}
              {#if item.subtitles.length}<div><dt>Sous-titres</dt><dd>{item.subtitles.map(trackName).join(' · ')}</dd></div>{/if}
            </dl>
          {/if}
        </div>
      </div>
    </div>
    {#if seasons.length}
      <section class="film-body">
        <div class="film-eps-head">
          <h2 class="v-titre">Épisodes</h2>
          {#if seasons.length > 1}
            <div class="v-seg film-seasons" role="group" aria-label="Saison">
              {#each seasons as s, i (s.season.id)}<button aria-pressed={i === shown} onclick={() => (shown = i)}>{s.season.title}</button>{/each}
            </div>
          {/if}
        </div>
        <ol class="film-episodes">
          {#each episodes as e (e.id)}
            {@const cur = e.id === next?.id && !!e.progress && !e.progress.seen}
            <li>
              <a class="film-episode" class:cur href={hrefs.films('watch', e.id)}>
                <span class="film-episode-n v-mono">{e.episode ?? ''}</span>
                {#if e.poster}
                  <span class="film-episode-art"><FilmPoster id={e.id} title={e.title} image={e.poster} width={240} /><span class="film-episode-play" aria-hidden="true"><Icon name="lecture" size={16} /></span></span>
                {:else}
                  <span class="film-episode-play" aria-hidden="true"><Icon name="lecture" size={16} /></span>
                {/if}
                <span class="grow film-episode-body">
                  <strong class="truncate">{e.title}</strong>
                  {#if e.progress?.seen}<span class="v-note film-seen"><Icon name="ok" size={14} /> Vu</span>
                  {:else if e.progress}<span class="v-mono film-left">{remaining(e.progress)}</span><span class="film-bar"><span style="width:{percent(e.progress)}"></span></span>
                  {:else}<span class="v-note">Pas encore vu</span>{/if}
                </span>
                <span class="v-mono v-mute">{shortDuration(e.duration)}</span>
              </a>
            </li>
          {/each}
        </ol>
      </section>
    {/if}
  </div>
  {#if picker && play}
    <WatchPicker links={[`film:${play.id}`]} kind="cinema" position={resumeAt(play)} onclose={() => (picker = false)} />
  {/if}
{:else if failed}
  <div class="film-page"><div class="film-missing"><Icon name="alerte" /><span class="grow">Ce titre est indisponible.</span><a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.films()}>Retour à la bibliothèque</a></div></div>
{:else}
  <div class="film-page"><Loader /></div>
{/if}
