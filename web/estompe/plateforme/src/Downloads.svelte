<script lang="ts">
  import { installeur, isAndroidBrowser } from './browser'
</script>

{#if isAndroidBrowser()}
  <a class="plbls-button plbls-button--discret plbls-button--block" href={installeur('Estompe.apk')} download>Télécharger l'application Android</a>
{:else}
  <a class="plbls-button plbls-button--discret plbls-button--block" href={installeur('Estompe-setup.exe')} download>Télécharger l'application Windows</a>
{/if}
