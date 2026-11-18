<script lang="ts">
  import { films } from './films.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { entracte } from './entracte.svelte'
  import { rubriqueActive, rubriques, TMDB_ATTRIBUTION } from './entracte'
  import Icon from '../../components/Icon.svelte'

  const route = $derived(router.current)
  const section = $derived(route.name === 'films' ? route.section : undefined)
  const droits = $derived(app.entracteMe?.permissions)
  const links = $derived(rubriques(droits, films))
  const notifier = async () => {
    if (await entracte.enablePush().catch(() => false)) notify.ok('Tu seras prévenu de l’avancée de tes demandes.')
    else notify.fail('Les notifications ne sont pas disponibles sur cet appareil ou ont été refusées.')
  }
</script>

<div class="side-head"><h2>Entracte</h2></div>
<div class="side-list">
  <p class="v-note film-sub">Films et séries</p>
  {#each links as l (l.label)}
    {@const actif = rubriqueActive(l.id, section)}
    {#if l.id === 'admin'}<hr class="film-nav-sep" />{/if}
    <a class="nav-item" class:active={actif} href={hrefs.films(l.id)} aria-current={actif ? 'page' : undefined}>
      <Icon name={l.icon} size={18} />
      <span class="grow truncate">{l.label}</span>
      {#if l.alerte}<span class="film-nav-alerte" aria-label={`${l.count} en attente`}>{l.count}</span>
      {:else if l.count}<span class="v-mono v-mute">{l.count}</span>{/if}
    </a>
  {/each}
  {#if droits?.request && entracte.push === 'off'}
    <button class="plbls-button plbls-button--compact plbls-button--contour film-notif" onclick={() => void notifier()}><Icon name="cloche" size={16} /> Être prévenu de mes demandes</button>
  {/if}
  {#if app.phase !== 'sections'}<p class="v-well v-note film-hint">Rejoins un salon cinéma pour lancer une séance à plusieurs.</p>{/if}
  <p class="v-note film-tmdb">{TMDB_ATTRIBUTION}</p>
</div>

<style>
  .film-sub { margin: 0 0 var(--espace-2) var(--espace-2); }
  .film-notif { margin: var(--espace-3) var(--espace-2) 0; }
  .film-tmdb { margin: var(--espace-4) var(--espace-2) 0; font-size: 0.75rem; }
  .film-nav-sep { height: 1px; margin: var(--espace-2); border: 0; background: var(--trait-fort); }
  .film-nav-alerte { min-width: 20px; padding: 1px 6px; border-radius: var(--r-p); background: var(--rose-texte); color: var(--surface); font-size: 12px; font-weight: 600; text-align: center; }
</style>
