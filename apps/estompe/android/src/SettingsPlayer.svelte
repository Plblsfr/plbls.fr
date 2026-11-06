<script lang="ts">
  import { prefs } from '../../frontend/src/lib/prefs.svelte'
</script>

<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.floatingPlayer} onchange={() => prefs.save()} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Lecteur flottant quand je quitte l'application (Regarder ensemble, Music2gether, écoute seul)</span>
</label>
