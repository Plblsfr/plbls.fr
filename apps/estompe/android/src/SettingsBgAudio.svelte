<script lang="ts">
  import { bgAudio, setBgAudio } from './bgaudio.svelte'
</script>

<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={bgAudio.on} onchange={(e) => setBgAudio(e.currentTarget.checked)} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Son de 2Gether en arrière-plan, écran éteint (expérimental)</span>
</label>
<p class="v-note">Garde le lecteur actif quand tu quittes l'application, sans fenêtre flottante, avec une notification de lecture. Ce mode est contraire aux conditions du lecteur intégré de YouTube (lecteur visible) : YouTube peut mettre la lecture en pause ou limiter le lecteur, et la batterie est davantage sollicitée. La notification indique si la lecture est interrompue. Réglage propre à cet appareil ; l'image dans l'image reste le mode conseillé.</p>
