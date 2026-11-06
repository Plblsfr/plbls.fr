<script lang="ts">
  import { films } from './films.svelte'
  import { router, hrefs } from '../../lib/route.svelte'

  const route = $derived(router.current)
  const section = $derived(route.name === 'films' ? route.section : undefined)
  const links = $derived([
    { id: undefined, label: 'Accueil', count: 0 },
    { id: 'movies', label: 'Films', count: films.movies },
    { id: 'series', label: 'Séries', count: films.series },
    { id: 'continue', label: 'Reprendre', count: films.resume.length },
    { id: 'seen', label: 'Déjà vus', count: films.seen },
  ])
</script>

<div class="side-head"><h2>Films et séries</h2></div>
<div class="side-list">
  {#each links as l (l.label)}
    <a class="nav-item" class:active={section === l.id} href={hrefs.films(l.id)} aria-current={section === l.id ? 'page' : undefined}>
      <span class="grow truncate">{l.label}</span>
      {#if l.count}<span class="small muted">{l.count}</span>{/if}
    </a>
  {/each}
  <p class="small muted film-hint">Rejoignez un salon cinéma pour lancer une séance à plusieurs.</p>
</div>
