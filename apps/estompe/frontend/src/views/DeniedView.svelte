<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { logout } from '../lib/auth'
</script>

<div class="center welcome">
  <div class="card">
    <h1>Accès non accordé</h1>
    <p class="muted">Ton compte est connecté, mais il n'a pas la permission d'utiliser Estompe. Demande à l'administrateur de te l'accorder, puis réessaie.</p>
    <button class="plbls-button plbls-button--block" onclick={() => app.boot()}>Réessayer</button>
    <button class="plbls-button plbls-button--lien" onclick={() => logout()}>Se déconnecter</button>
  </div>
</div>
