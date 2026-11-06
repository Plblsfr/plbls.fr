<script lang="ts">
  import { LOGIN_URL } from "./api";
  import { gate } from "./lib/state.svelte";
  import Apps from "./pages/Apps.svelte";
  import Assignments from "./pages/Assignments.svelte";
  import Audit from "./pages/Audit.svelte";

  const PAGES = { apps: "Applications", assignments: "Attributions", audit: "Journal" } as const;
  type Page = keyof typeof PAGES;

  const fromHash = (): Page => (location.hash.slice(1) in PAGES ? (location.hash.slice(1) as Page) : "apps");
  let page = $state<Page>(fromHash());

  const closeMenu = () => {
    const side = document.querySelector(".plbls-sidebar.is-open");
    if (!side) return;
    side.classList.remove("is-open");
    const b = side.querySelector(".plbls-sidebar__bascule");
    b?.setAttribute("aria-expanded", "false");
    if (b) b.textContent = "Menu";
  };
  const onHash = () => { page = fromHash(); closeMenu(); };
  const onClick = (e: MouseEvent) => { if (!(e.target as Element).closest(".plbls-sidebar")) closeMenu(); };
  const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") closeMenu(); };
</script>

<svelte:window onhashchange={onHash} onclick={onClick} onkeydown={onKey} />

{#if gate.value}
  <div class="app-main app-gate">
    <div class="plbls-empty-state">
      {#if gate.value === "login"}
        <h3>Session absente ou expirée</h3>
        <p>Connecte-toi pour gérer les permissions.</p>
        <a class="plbls-button" href={LOGIN_URL}>Se connecter</a>
      {:else}
        <h3>Accès réservé</h3>
        <p>Cette interface est réservée aux administrateurs des permissions.</p>
      {/if}
    </div>
  </div>
{:else}
  <div class="app-shell">
    <div class="plbls-sidebar">
      <button type="button" class="plbls-sidebar__bascule" aria-expanded="false">Menu</button>
      <nav class="plbls-sidebar__colonne" aria-label="Navigation principale">
        <a class="plbls-sidebar__marque" href="#apps">permission</a>
        <div class="plbls-sidebar__liens">
          {#each Object.keys(PAGES) as Page[] as p (p)}
            <a class="plbls-sidebar__lien{p === page ? ' is-active' : ''}" href="#{p}" aria-current={p === page ? "page" : undefined}>{PAGES[p]}</a>
          {/each}
        </div>
      </nav>
    </div>
    <main class="app-main">
      {#if page === "apps"}<Apps />{/if}
      {#if page === "assignments"}<Assignments />{/if}
      {#if page === "audit"}<Audit />{/if}
    </main>
  </div>
{/if}
