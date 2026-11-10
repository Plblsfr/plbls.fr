<script lang="ts">
  import { films } from './films.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { entracteOn } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { entracte } from '../../sections/entracte/entracte.svelte'
  import { TMDB_ATTRIBUTION } from '../../sections/entracte/entracte'
  import Icon, { type IconName } from '../../components/Icon.svelte'

  const route = $derived(router.current)
  const section = $derived(route.name === 'films' ? route.section : undefined)
  const avecEntracte = entracteOn()
  const droits = $derived(app.entracteMe?.permissions)
  const links: { id: string | undefined; label: string; icon: IconName; count: number }[] = $derived([
    { id: undefined, label: 'Accueil', icon: 'maison', count: 0 },
    { id: 'movies', label: 'Films', icon: 'film', count: films.movies },
    { id: 'series', label: 'Séries', icon: 'tele', count: films.series },
    { id: 'continue', label: 'Reprendre', icon: 'replay', count: films.resume.length },
    { id: 'seen', label: avecEntracte ? 'Historique' : 'Déjà vus', icon: 'ok', count: films.seen },
    ...(avecEntracte ? [{ id: 'search', label: 'Rechercher et demander', icon: 'cherche' as IconName, count: 0 }] : []),
    ...(avecEntracte && droits?.request ? [{ id: 'requests', label: 'Mes demandes', icon: 'bulle' as IconName, count: 0 }] : []),
    ...(avecEntracte && droits?.admin ? [{ id: 'admin', label: 'Administration', icon: 'bouclier' as IconName, count: 0 }] : []),
  ])
  const notifier = async () => {
    if (await entracte.enablePush().catch(() => false)) notify.ok('Tu seras prévenu de l’avancée de tes demandes.')
    else notify.fail('Les notifications ne sont pas disponibles sur cet appareil ou ont été refusées.')
  }
</script>

<div class="side-head"><h2>{avecEntracte ? 'Entracte' : 'Films et séries'}</h2></div>
<div class="side-list">
  {#if avecEntracte}<p class="v-note film-sub">Films et séries</p>{/if}
  {#each links as l (l.label)}
    <a class="nav-item" class:active={section === l.id} href={hrefs.films(l.id)} aria-current={section === l.id ? 'page' : undefined}>
      <Icon name={l.icon} size={18} />
      <span class="grow truncate">{l.label}</span>
      {#if l.count}<span class="v-mono v-mute">{l.count}</span>{/if}
    </a>
  {/each}
  {#if avecEntracte && droits?.request && entracte.push === 'off'}
    <button class="plbls-button plbls-button--compact plbls-button--contour film-notif" onclick={() => void notifier()}><Icon name="cloche" size={16} /> Être prévenu de mes demandes</button>
  {/if}
  {#if app.phase !== 'entracte'}<p class="v-well v-note film-hint">Rejoins un salon cinéma pour lancer une séance à plusieurs.</p>{/if}
  {#if avecEntracte}<p class="v-note film-tmdb">{TMDB_ATTRIBUTION}</p>{/if}
</div>

<style>
  .film-sub { margin: 0 0 var(--espace-2) var(--espace-2); }
  .film-notif { margin: var(--espace-3) var(--espace-2) 0; }
  .film-tmdb { margin: var(--espace-4) var(--espace-2) 0; font-size: 0.75rem; }
</style>
