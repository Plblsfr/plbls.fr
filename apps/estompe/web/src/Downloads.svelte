<script lang="ts">
  import { isAndroidBrowser } from './browser'
</script>

{#if isAndroidBrowser()}
  <a class="plbls-button plbls-button--discret plbls-button--block" href="/updates/Estompe.apk" download>Télécharger l'application Android</a>
{:else}
  <a class="plbls-button plbls-button--discret plbls-button--block" href="/updates/Estompe-setup.exe" download>Télécharger l'application Windows</a>
{/if}
