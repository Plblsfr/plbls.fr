<script lang="ts">
  import { prefs } from '../../frontend/src/lib/prefs.svelte'
</script>

<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.shareActivity} onchange={() => prefs.save()} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Afficher le jeu en cours aux autres (toi seul le vois sinon)</span>
</label>
