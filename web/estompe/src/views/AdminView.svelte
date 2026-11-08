<script lang="ts">
  import AdminSpace from '../components/AdminSpace.svelte'
  import Icon from '../components/Icon.svelte'
  import { adminSection } from '../lib/admin'
  import { logout } from '../lib/auth'
  import { router, hrefs } from '../lib/route.svelte'
  import { app } from '../lib/store.svelte'

  let nav = $state(false)
  const route = $derived(router.current)
  const section = $derived(route.name === 'admin' ? route.section : undefined)
  const actif = $derived(adminSection(section, app.adminSections))
  $effect(() => { if (route.name !== 'admin') router.go(hrefs.admin(actif)) })
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && nav) nav = false }} />
<div class="app adm-seule" data-nav={nav ? 'open' : 'closed'}>
  <div class="nav-drawer" role="presentation" onclick={(e) => { if ((e.target as HTMLElement).closest('a')) nav = false }}>
    <aside class="side">
      <div class="side-head"><h2>Administration</h2></div>
      <div class="side-list">
        <div class="side-title">Sections</div>
        {#each app.adminSections as s (s.id)}
          <a class="nav-item" class:active={actif === s.id} href={hrefs.admin(s.id)} aria-current={actif === s.id ? 'page' : undefined}>
            <Icon name={s.icon} size={16} /><span class="truncate">{s.label}</span>
          </a>
        {/each}
      </div>
      <div class="adm-seule-pied">
        <p class="v-note">Ton compte n'a pas accès à la messagerie d'Estompe : seule l'administration est ouverte.</p>
        <button class="plbls-button plbls-button--lien" onclick={() => logout()}>Se déconnecter</button>
      </div>
    </aside>
  </div>
  <button class="scrim" aria-label="Fermer le menu" onclick={() => (nav = false)}></button>
  <main class="main" id="main" tabindex="-1">
    <AdminSpace {section} onmenu={() => (nav = true)} />
  </main>
</div>

<style>
  .adm-seule { grid-template-columns: 272px minmax(0, 1fr); }
  .adm-seule-pied { display: flex; flex-direction: column; gap: 8px; padding: 12px 18px 18px; box-shadow: 0 -1px 0 var(--trait); }
  .adm-seule-pied p { margin: 0; }
  @media (max-width: 900px) { .adm-seule { grid-template-columns: minmax(0, 1fr); } }
</style>
