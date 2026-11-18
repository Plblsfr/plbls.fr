<script lang="ts">
  import Icon from '../../components/Icon.svelte'

  let { message, onretry }: { message: string; onretry: () => void } = $props()
</script>

<div class="sab-vide" role="alert">
  <Icon name="alerte" size={28} />
  <h2 class="sab-h2">Les stats ne répondent pas</h2>
  <p>{message} Le reste de l’app fonctionne normalement.</p>
  <button class="plbls-button" onclick={onretry}><Icon name="actualise" size={18} />Réessayer</button>
</div>
