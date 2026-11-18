<script lang="ts">
  import FilmsNav from '../sections/entracte/FilmsNav.svelte'
  import FilmsSpace from '../sections/entracte/FilmsSpace.svelte'
  import SillonNav from '../sections/sillon/SillonNav.svelte'
  import SillonSpace from '../sections/sillon/SillonSpace.svelte'
  import SillonMini from '../sections/sillon/SillonMini.svelte'
  import SablierNav from '../sections/sablier/SablierNav.svelte'
  import SablierSpace from '../sections/sablier/SablierSpace.svelte'
  import { sablier } from '../sections/sablier/sablier.svelte'
  import { lecteur } from '../sections/sillon/sillon.svelte'
  import Icon from '../components/Icon.svelte'
  import { router, hrefs } from '../lib/route.svelte'
  import { logout } from '../lib/auth'
  import { app } from '../lib/store.svelte'

  let nav = $state(false)
  const route = $derived(router.current)
  const films = $derived(route.name === 'films' && app.movies)
  const musique = $derived(route.name === 'musique' && app.musique)
  const stats = $derived(route.name === 'stats' && app.stats)
  const nom = $derived(app.entracteMe?.name ?? app.sillonMe?.name ?? '')
  $effect(() => { if (!films && !musique && !stats) router.go(app.movies ? hrefs.films() : hrefs.musique()) })
</script>

<div class="app sections-seules" data-nav={nav ? 'open' : 'closed'}>
  <div class="nav-drawer" role="presentation" onclick={(e) => { if ((e.target as HTMLElement).closest('a')) nav = false }}>
    <aside class="side">
      {#if stats}<SablierNav />{:else if musique}<SillonNav />{:else}<FilmsNav />{/if}
      {#if Number(app.movies) + Number(app.musique) + Number(app.stats) > 1}
        <nav class="sections-choix" aria-label="Sections">
          {#if app.movies}<a class="nav-item" class:active={films} href={hrefs.films()} aria-current={films ? 'page' : undefined}><Icon name="film" size={18} /><span class="grow truncate">Entracte · films et séries</span></a>{/if}
          {#if app.musique}<a class="nav-item" class:active={musique} href={hrefs.musique()} aria-current={musique ? 'page' : undefined}><Icon name="note" size={18} /><span class="grow truncate">Sillon · musique</span></a>{/if}
          {#if app.stats}<a class="nav-item" class:active={stats} href={hrefs.stats()} aria-current={stats ? 'page' : undefined}><Icon name="sablier" size={18} /><span class="grow truncate">Stats</span>{#if sablier.pastille}<span class="sab-pip" role="img" aria-label="Du nouveau dans Stats"></span>{/if}</a>{/if}
        </nav>
      {/if}
      <div class="dock-w">
        {#if lecteur.piste}<div class="dock"><SillonMini /></div>{/if}
        <div class="sections-moi">
          <span class="v-note grow truncate">{nom}</span>
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => logout()}><Icon name="sortie" size={14} /> Se déconnecter</button>
        </div>
      </div>
    </aside>
  </div>
  <button class="scrim" aria-label="Fermer le menu" onclick={() => (nav = false)}></button>
  <main class="main" id="main" tabindex="-1">
    {#if stats}
      <SablierSpace section={route.name === 'stats' ? route.section : undefined} id={route.name === 'stats' ? route.id : undefined} onmenu={() => (nav = true)} />
    {:else if musique}
      <SillonSpace section={route.name === 'musique' ? route.section : undefined} id={route.name === 'musique' ? route.id : undefined} onmenu={() => (nav = true)} />
    {:else if films}
      <FilmsSpace section={route.name === 'films' ? route.section : undefined} id={route.name === 'films' ? route.id : undefined} onmenu={() => (nav = true)} />
    {/if}
    {#if lecteur.piste}<div class="sillon-bande"><SillonMini bande /></div>{/if}
  </main>
</div>

<style>
  @media (min-width: 900.02px) { :global(.app.sections-seules) { grid-template-columns: 272px minmax(0, 1fr); } }
  .sections-moi { display: flex; align-items: center; gap: var(--espace-2); padding: var(--espace-2) var(--espace-1) 0; }
  .sections-choix { display: grid; gap: 1px; padding: var(--espace-2) 10px; box-shadow: inset 0 1px 0 var(--trait); }
  .sections-seules .side { display: flex; flex-direction: column; }
  .sections-seules .side :global(.side-list) { flex: 1; min-height: 0; overflow-y: auto; }
</style>
