<script lang="ts">
  import { installeur, isAndroidBrowser } from './browser'
</script>

{#if isAndroidBrowser()}
  <p class="muted small">APK à installer à la main (autoriser les sources inconnues) ; une fois installée, l'app propose elle-même les mises à jour.</p>
  <a class="plbls-button plbls-button--compact plbls-button--contour" href={installeur('Estompe.apk')} download>Télécharger pour Android</a>
{:else}
  <p class="muted small">Notifications natives, icône dans la zone de notification et mises à jour automatiques.</p>
  <a class="plbls-button plbls-button--compact plbls-button--contour" href={installeur('Estompe-setup.exe')} download>Télécharger pour Windows</a>
{/if}
