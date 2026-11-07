<script lang="ts">
  import { films } from './films.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import Icon, { type IconName } from '../../components/Icon.svelte'

  const route = $derived(router.current)
  const section = $derived(route.name === 'films' ? route.section : undefined)
  const links: { id: string | undefined; label: string; icon: IconName; count: number }[] = $derived([
    { id: undefined, label: 'Accueil', icon: 'maison', count: 0 },
    { id: 'movies', label: 'Films', icon: 'film', count: films.movies },
    { id: 'series', label: 'Séries', icon: 'tele', count: films.series },
    { id: 'continue', label: 'Reprendre', icon: 'replay', count: films.resume.length },
    { id: 'seen', label: 'Déjà vus', icon: 'ok', count: films.seen },
  ])
</script>

<div class="side-head"><h2>Films et séries</h2></div>
<div class="side-list">
  {#each links as l (l.label)}
    <a class="nav-item" class:active={section === l.id} href={hrefs.films(l.id)} aria-current={section === l.id ? 'page' : undefined}>
      <Icon name={l.icon} size={18} />
      <span class="grow truncate">{l.label}</span>
      {#if l.count}<span class="v-mono v-mute">{l.count}</span>{/if}
    </a>
  {/each}
  <p class="v-well v-note film-hint">Rejoins un salon cinéma pour lancer une séance à plusieurs.</p>
</div>
