<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import { app } from '../lib/store.svelte'

  let { onmenu, oncreate }: { onmenu: () => void; oncreate: () => void } = $props()
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1>Accueil</h1>
</header>
<div class="center">
  <div class="plbls-empty-state">
    {#if app.servers.length === 0}
      <h3>Bienvenue, {app.pseudo}</h3>
      <p>Tu n'es dans aucun serveur. Crée le tien, ou rejoins celui d'un ami avec son lien ou son code d'invitation.</p>
      <button class="plbls-button" onclick={oncreate}>Créer ou rejoindre un serveur</button>
    {:else}
      <h3>Choisis une conversation</h3>
      <p>Sélectionne un serveur à gauche, ou ouvre un message privé.</p>
    {/if}
  </div>
</div>
