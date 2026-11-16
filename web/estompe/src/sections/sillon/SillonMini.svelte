<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import MiniProgress from '../../components/MiniProgress.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import SillonCover from './SillonCover.svelte'
  import SillonLike from './SillonLike.svelte'
  import { lecteur } from './sillon.svelte'
  import { marqueDe, REPETITIONS } from './sillon'

  let { bande = false }: { bande?: boolean } = $props()

  const p = $derived(lecteur.piste)
  let volumeOuvert = $state(false)
  const repetition = $derived(REPETITIONS.find((r) => r.id === lecteur.repetition)?.label ?? '')
  const pct = $derived(lecteur.duree ? Math.min(100, (lecteur.position / lecteur.duree) * 100) : 0)
</script>

{#if p}
  <div class="lect sillon-lect" class:bande role="group" aria-label="Sillon, lecture en cours">
    <div class="lect-l">
      <a class="lect-pochette" href={hrefs.musique('album', p.album.id)} aria-label={`Album ${p.album.title}`}><SillonCover id={p.album.id} title={p.album.title} image={p.album.cover} size="small" /></a>
      <span class="lect-t"><span class="lect-titre">{p.title}</span><span class="lect-sous">{p.artist.name}{#if marqueDe(lecteur.element)} · {marqueDe(lecteur.element)}{/if}{#if lecteur.attente} · Chargement…{/if}</span></span>
      {#if bande}<span class="lect-like"><SillonLike kind="tracks" id={p.id} titre={p.title} /></span>{/if}
      {#if !bande}<button class="ib lect-ib" onclick={() => lecteur.precedent()} aria-label="Précédent" title="Précédent"><Icon name="precedent" size={16} /></button>{/if}
      <button class="ib lect-ib" onclick={() => lecteur.basculer()} aria-label={lecteur.lecture ? 'Pause' : 'Lecture'} title={lecteur.lecture ? 'Pause' : 'Lecture'}><Icon name={lecteur.lecture ? 'pause' : 'play'} size={16} /></button>
      <button class="ib lect-ib" onclick={() => lecteur.suivant()} disabled={!lecteur.peutSuivre} aria-label="Suivant" title="Suivant"><Icon name="next" size={16} /></button>
    </div>
    {#if lecteur.bloque}<button class="v-btn sm rose" onclick={() => lecteur.debloquer()}><Icon name="volmax" size={15} />Activer le son</button>{/if}
    {#if lecteur.erreur}<p class="lect-err" role="alert">{lecteur.erreur}</p>{/if}
    {#if bande}
      <span class="sillon-barre" aria-hidden="true"><span style="width:{pct}%"></span></span>
    {:else}
      <MiniProgress pos={lecteur.position} total={lecteur.duree} onseek={(s) => lecteur.deplacer(s)} />
      <div class="lect-l">
        <span class="grow v-mono lect-file">{lecteur.suivante ? `À suivre : ${lecteur.suivante.title}` : 'Fin de la file'}</span>
        <span class="lect-like"><SillonLike kind="tracks" id={p.id} titre={p.title} /></span>
        <button class="ib lect-ib" class:on={lecteur.aleatoire} aria-pressed={lecteur.aleatoire} onclick={() => void lecteur.basculerAleatoire().catch((e) => notify.error(e, 'La file n’a pas pu être modifiée'))} aria-label="Aléatoire" title="Aléatoire"><Icon name="shuffle" size={16} /></button>
        <button class="ib lect-ib lect-repete" class:on={lecteur.repetition !== 'off'} aria-pressed={lecteur.repetition !== 'off'} onclick={() => void lecteur.changerRepetition().catch((e) => notify.error(e, 'La file n’a pas pu être modifiée'))} aria-label={`Répétition : ${repetition}`} title={repetition}><Icon name="repeat" size={16} />{#if lecteur.repetition === 'one'}<span class="lect-un" aria-hidden="true">1</span>{/if}</button>
        <a class="ib lect-ib" href={hrefs.musique('queue')} aria-label="File de lecture" title="File de lecture"><Icon name="list" size={16} /></a>
        {#if volumeOuvert}<input class="lect-volume" type="range" min="0" max="100" value={Math.round(lecteur.volume * 100)} oninput={(e) => lecteur.regler(Number(e.currentTarget.value) / 100)} aria-label="Volume de Sillon" />{/if}
        <button class="ib lect-ib" onclick={() => (volumeOuvert = !volumeOuvert)} aria-expanded={volumeOuvert} aria-label="Volume de Sillon" title="Volume"><Icon name="volmax" size={16} /></button>
        <button class="ib lect-ib" onclick={() => lecteur.arreter()} aria-label="Arrêter la musique" title="Arrêter"><Icon name="x" size={16} /></button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .lect { display: grid; gap: 4px; min-width: 0; font-size: 13px; }
  .lect-l { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .lect-pochette { position: relative; flex: none; width: 36px; height: 36px; margin-right: 6px; overflow: hidden; border-radius: var(--r-s); }
  .lect-t { display: grid; flex: 1; min-width: 0; line-height: 1.25; }
  .lect-titre { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lect-sous { font-size: 11.5px; opacity: 0.72; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lect-ib { width: 32px; height: 32px; color: inherit; }
  .lect .v-btn.sm { height: 30px; }
  .lect-ib.on { color: var(--rose-fonce); }
  .lect-like :global(.v-ib) { color: inherit; }
  .lect-like :global(.v-ib[aria-pressed='true']) { color: var(--rose-fonce); }
  .lect-repete { position: relative; }
  .lect-un { position: absolute; top: 3px; right: 2px; min-width: 12px; height: 12px; border-radius: 6px; background: var(--rose-fonce); color: var(--papier); font-size: 9px; font-weight: 700; line-height: 12px; text-align: center; }
  .lect-err { margin: 0; font-size: 12px; color: var(--danger); }
  .lect-file { min-width: 0; overflow: hidden; font-size: 10.5px; opacity: 0.72; white-space: nowrap; text-overflow: ellipsis; }
  .lect-volume { width: 0; flex: 1 1 80px; min-width: 60px; accent-color: var(--rose-fonce); }
  .lect :global(.mini-progress) { padding: 0; gap: 6px; }
  .lect :global(.mini-time) { color: inherit; opacity: 0.72; font-family: var(--f-mono); font-size: 10.5px; }
  .lect :global(.mini-bar::-webkit-slider-runnable-track) { background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .lect :global(.mini-bar::-moz-range-track) { background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .lect :global(.mini-bar::-webkit-slider-thumb) { background: currentColor; }
  .lect :global(.mini-bar::-moz-range-thumb) { background: currentColor; }
  .lect.bande { position: relative; gap: 6px; padding: 8px 10px 10px; background: var(--dock-fond); color: var(--dock-texte); box-shadow: 0 -1px 0 var(--dock-trait); }
  .sillon-barre { position: absolute; left: 0; right: 0; bottom: 0; height: 3px; background: color-mix(in srgb, currentColor 18%, transparent); }
  .sillon-barre > span { display: block; height: 100%; background: var(--rose-fonce); }
  @media (pointer: coarse) { .lect-ib { width: 44px; height: 44px; } .lect .v-btn.sm { min-height: 44px; } }
</style>
