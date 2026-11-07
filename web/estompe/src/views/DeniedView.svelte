<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { logout } from '../lib/auth'
</script>

<div class="acces">
  <div class="acces-carte">
    <span class="acces-pastille att" aria-hidden="true"><Icon name="cadenas" size={22} /></span>
    <h1>Accès non accordé</h1>
    <p class="acces-texte">Ton compte est connecté, mais il n'a pas la permission d'utiliser Estompe. Demande à l'administrateur de te l'accorder, puis réessaie.</p>
    <button class="plbls-button plbls-button--block" onclick={() => app.boot()}><Icon name="actualise" size={18} />Réessayer</button>
    <button class="plbls-button plbls-button--lien acces-second" onclick={() => logout()}>Se déconnecter</button>
  </div>
</div>
