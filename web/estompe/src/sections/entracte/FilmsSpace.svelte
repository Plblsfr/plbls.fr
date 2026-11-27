<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import FilmDetail from './FilmDetail.svelte'
  import FilmPlayer from './FilmPlayer.svelte'
  import FilmHistory from './FilmHistory.svelte'
  import FilmBadges from './FilmBadges.svelte'
  import FilmsLibrary from './FilmsLibrary.svelte'
  import EntracteSearch from './EntracteSearch.svelte'
  import EntracteAdmin from './EntracteAdmin.svelte'
  import { app } from '../../lib/store.svelte'
  import { hrefs, router } from '../../lib/route.svelte'
  import { films } from './films.svelte'
  import { rubriqueActive, rubriques } from './entracte'

  let { section, id, onmenu }: { section?: string; id?: string; onmenu: () => void } = $props()

  const droits = $derived(app.entracteMe?.permissions)
  const links = $derived(rubriques(droits, films))
  const demandes = $derived(section === 'search' || (section === 'requests' && !!droits?.request))

  function chercher(texte: string) {
    films.query = texte
    if (texte.trim() && !demandes) router.go(hrefs.films('search'))
  }

  let avant = false
  $effect(() => {
    if (avant && !demandes) films.query = ''
    avant = demandes
  })
  $effect(() => { if (section !== 'watch') void films.refresh(!!droits?.admin).catch(() => {}) })
</script>

{#if section === 'watch' && id}
  <FilmPlayer {id} />
{:else}
  <header class="main-head film-head">
    <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
    <h1>Entracte</h1>
    <label class="film-search film-search--tete">
      <Icon name="cherche" size={16} />
      <span class="v-sr">Rechercher un film ou une série</span>
      <input class="plbls-input" type="search" placeholder="Un film, une série, même absent du serveur…" maxlength="100" value={films.query} oninput={(e) => chercher(e.currentTarget.value)} />
    </label>
  </header>
  <nav class="film-rubriques" aria-label="Rubriques d’Entracte">
    {#each links as l (l.label)}
      {@const actif = rubriqueActive(l.id, section)}
      <a class="film-rubrique" class:active={actif} href={hrefs.films(l.id)} aria-current={actif ? 'page' : undefined}>{l.label}{#if l.alerte}<span class="film-rubrique-n">{l.count}</span>{/if}</a>
    {/each}
  </nav>
  {#if section === 'item' && id}
    <FilmDetail {id} />
  {:else if demandes}
    <EntracteSearch />
  {:else if section === 'admin' && droits?.admin}
    <EntracteAdmin />
  {:else if section === 'seen'}
    <FilmHistory />
  {:else if section === 'badges'}
    <FilmBadges />
  {:else}
    <FilmsLibrary {section} />
  {/if}
{/if}
