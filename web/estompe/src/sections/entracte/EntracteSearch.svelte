<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import FilmPoster from './FilmPoster.svelte'
  import EntractePoster from './EntractePoster.svelte'
  import EntracteRequests from './EntracteRequests.svelte'
  import RequestForm from './RequestForm.svelte'
  import { entracteApi } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { films } from './films.svelte'
  import { splitSearch, stateLabel, stateTone, TMDB_ATTRIBUTION, type SearchResult } from './entracte'

  let items = $state<SearchResult[]>([])
  let tmdbError = $state(false)
  let loading = $state(false)
  let asked = $state('')
  let turn = 0
  let version = $state(0)

  const canRequest = $derived(!!app.entracteMe?.permissions.request)
  const groups = $derived(splitSearch(items))
  const text = $derived(films.query.trim())

  $effect(() => {
    const wanted = text
    void version
    if (!wanted) { items = []; return }
    const mine = ++turn
    const t = setTimeout(async () => {
      loading = true
      try {
        const r = await entracteApi.search(wanted)
        if (mine !== turn) return
        items = r.items
        tmdbError = r.tmdb_error
      } catch (e) { if (mine === turn) notify.error(e, 'La recherche est indisponible') } finally { if (mine === turn) loading = false }
    }, 300)
    return () => clearTimeout(t)
  })

  const key = (i: SearchResult) => `${i.kind}-${i.tmdb_id}`
  const meta = (i: SearchResult) => [i.kind === 'series' ? 'Série' : 'Film', i.year].filter(Boolean).join(' · ')
  const pending = (i: SearchResult) => !!i.request && (i.request.state === 'en_attente' || i.request.state === 'acceptee')
  const toggle = (i: SearchResult) => { asked = asked === key(i) ? '' : key(i) }
  const done = () => { version++ }
</script>

<div class="film-page">
  <div class="dem">
    <div class="dem-resultats">
      {#if tmdbError}<p class="v-note" role="status">La recherche des titres absents est indisponible pour l’instant : seuls les titres du serveur sont affichés.</p>{/if}
      {#if !text}
        <div class="v-empty"><Icon name="cherche" size={28} /><p class="v-note">Cherche un film ou une série dans le champ en haut de la page{canRequest ? ' : s’il n’est pas sur le serveur, tu pourras le demander.' : '.'}</p></div>
      {:else if loading && !items.length}
        <Loader />
      {:else if !items.length}
        <div class="v-empty"><Icon name="film" size={28} /><p class="v-note">Aucun titre ne correspond à « {text} ».</p></div>
      {/if}
      {#if groups.present.length}
        <section class="film-section">
          <h2 class="v-titre">Sur le serveur <span class="v-mono v-mute dem-n">{groups.present.length}</span></h2>
          <ul class="dem-liste">
            {#each groups.present as i (key(i))}
              <li class="dem-ligne" class:ouvert={asked === key(i)}>
                <div class="dem-tete">
                  <a class="dem-art" href={hrefs.films('item', i.id ?? '')} tabindex="-1" aria-hidden="true"><FilmPoster id={i.id ?? ''} title={i.title} image={!!i.poster} /></a>
                  <span class="dem-corps"><strong>{i.title}</strong><span class="v-mono v-mute">{meta(i)}</span></span>
                  {#if canRequest && i.kind === 'series'}<button class="plbls-button plbls-button--compact plbls-button--contour" aria-expanded={asked === key(i)} onclick={() => toggle(i)}>Demander des saisons</button>{/if}
                  <a class="plbls-button plbls-button--compact dem-ouvrir" href={hrefs.films('item', i.id ?? '')}><Icon name="lecture" size={14} /> Ouvrir</a>
                </div>
                {#if asked === key(i)}<RequestForm kind="series" tmdbId={i.tmdb_id} title={i.title} onclose={() => (asked = '')} ondone={done} />{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
      {#if groups.absent.length}
        <section class="film-section">
          <h2 class="v-titre">Pas encore sur le serveur <span class="v-mono v-mute dem-n">{groups.absent.length}</span></h2>
          <ul class="dem-liste">
            {#each groups.absent as i (key(i))}
              <li class="dem-ligne" class:ouvert={asked === key(i)}>
                <div class="dem-tete">
                  <span class="dem-art"><EntractePoster url={i.poster_url} title={i.title} seed={key(i)} /></span>
                  <span class="dem-corps"><strong>{i.title}</strong><span class="v-mono v-mute">{meta(i)}</span></span>
                  {#if pending(i) && i.request}
                    <span class="v-tag {stateTone(i.request.state) || 'att'}">Déjà demandé · {stateLabel(i.request.state).toLowerCase()}</span>
                    {#if canRequest}<button class="plbls-button plbls-button--compact plbls-button--contour" aria-expanded={asked === key(i)} onclick={() => toggle(i)}>Me prévenir aussi</button>{/if}
                  {:else if canRequest && i.requestable}
                    <button class="plbls-button plbls-button--compact" aria-expanded={asked === key(i)} onclick={() => toggle(i)}><Icon name="envoi" size={14} /> Demander</button>
                  {/if}
                </div>
                {#if asked === key(i)}<RequestForm kind={i.kind === 'series' ? 'series' : 'movie'} tmdbId={i.tmdb_id} title={i.title} onclose={() => (asked = '')} ondone={done} />{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
      <p class="v-note film-tmdb-note">{TMDB_ATTRIBUTION}</p>
    </div>
    {#if canRequest}<EntracteRequests {version} />{/if}
  </div>
</div>

<style>
  .dem { display: flex; flex-wrap: wrap; align-items: flex-start; gap: var(--espace-8); }
  .dem-resultats { flex: 2 1 480px; min-width: 0; display: flex; flex-direction: column; gap: var(--espace-6); }
  .dem-n { font-size: 13px; font-weight: 400; letter-spacing: 0; }
  .dem-liste { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .dem-ligne { display: flex; flex-direction: column; gap: var(--espace-4); padding: var(--espace-3) var(--espace-4) var(--espace-3) var(--espace-3); border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); }
  .dem-ligne.ouvert { padding-bottom: var(--espace-4); box-shadow: 0 0 0 1.5px var(--encre); }
  .dem-tete { display: flex; flex-wrap: wrap; align-items: center; gap: var(--espace-3) var(--espace-4); }
  .dem-art { flex: none; width: 56px; aspect-ratio: 2 / 3; overflow: hidden; border-radius: var(--r-s); }
  .dem-art :global(.film-art-title), .dem-art :global(.film-disc) { display: none; }
  .dem-corps { flex: 1 1 180px; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .dem-corps strong { font-size: 17px; overflow-wrap: anywhere; }
  .dem-ouvrir { text-decoration: none; }
  .film-tmdb-note { margin: 0; font-size: 0.75rem; }
</style>
