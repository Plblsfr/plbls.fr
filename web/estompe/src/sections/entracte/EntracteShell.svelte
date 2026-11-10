<script lang="ts">
  import FilmsNav from '../../blocks/films/FilmsNav.svelte'
  import FilmsSpace from '../../blocks/films/FilmsSpace.svelte'
  import Icon from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { logout } from '../../lib/auth'
  import { app } from '../../lib/store.svelte'

  let nav = $state(false)
  const route = $derived(router.current)
  $effect(() => { if (route.name !== 'films') router.go(hrefs.films()) })
</script>

<div class="app entracte-seule" data-nav={nav ? 'open' : 'closed'}>
  <div class="nav-drawer" role="presentation" onclick={(e) => { if ((e.target as HTMLElement).closest('a')) nav = false }}>
    <aside class="side">
      <FilmsNav />
      <div class="entracte-moi">
        <span class="v-note grow truncate">{app.entracteMe?.name ?? ''}</span>
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => logout()}><Icon name="sortie" size={14} /> Se déconnecter</button>
      </div>
    </aside>
  </div>
  <button class="scrim" aria-label="Fermer le menu" onclick={() => (nav = false)}></button>
  <main class="main" id="main" tabindex="-1">
    <FilmsSpace section={route.name === 'films' ? route.section : undefined} id={route.name === 'films' ? route.id : undefined} onmenu={() => (nav = true)} />
  </main>
</div>

<style>
  @media (min-width: 900.02px) { :global(.app.entracte-seule) { grid-template-columns: 272px minmax(0, 1fr); } }
  .entracte-moi { display: flex; align-items: center; gap: var(--espace-2); padding: var(--espace-3); box-shadow: inset 0 1px 0 var(--trait); }
  .entracte-seule .side { display: flex; flex-direction: column; }
  .entracte-seule .side :global(.side-list) { flex: 1; min-height: 0; overflow-y: auto; }
</style>
